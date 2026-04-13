use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, FrameExt as _, List, ListItem, ListState, Paragraph, Scrollbar,
    ScrollbarOrientation, ScrollbarState, Tabs, Wrap,
};

use super::model::{
    ActivePanel, AgentEditorMode, ChatRole, ComponentsTab, CreateAgentField, CreateAgentModal,
    CreateAgentTab, ImportModal, Modal, Model,
};

const BORDER_NORMAL: Color = Color::DarkGray;
const BORDER_FOCUSED: Color = Color::Cyan;
const SUBTLE: Color = Color::DarkGray;

fn panel_style(active: ActivePanel, this: ActivePanel) -> Style {
    if active == this {
        Style::default().fg(BORDER_FOCUSED)
    } else {
        Style::default().fg(BORDER_NORMAL)
    }
}

pub fn view(frame: &mut Frame, model: &mut Model) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(50),
            Constraint::Percentage(25),
        ])
        .split(frame.area());

    // Left column: Agents (top) + Chats (bottom).
    let left_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(columns[0]);

    render_agents_panel(frame, model, left_rows[0]);
    render_chats_panel(frame, model, left_rows[1]);
    render_center_panel(frame, model, columns[1]);
    render_components_panel(frame, model, columns[2]);

    if let Some(modal) = model.modal.as_mut() {
        render_modal(frame, modal);
    }
}

// ---------------------------------------------------------------------------
// Agents panel
// ---------------------------------------------------------------------------

fn render_agents_panel(frame: &mut Frame, model: &mut Model, area: Rect) {
    let style = panel_style(model.active, ActivePanel::Agents);

    let title = " Agents [↑↓] [Enter chat] [n new] [e edit] [d del] ";

    let agents_block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(style);

    if !model.env_loaded || model.agents_loading {
        let msg = Paragraph::new("Loading agents...")
            .style(Style::default().fg(SUBTLE))
            .block(agents_block);
        frame.render_widget(msg, area);
        return;
    }

    if let Some(err) = &model.agents_error {
        let msg = Paragraph::new(format!("Error: {err}\n\nCtrl+R to retry"))
            .style(Style::default().fg(Color::Red))
            .block(agents_block)
            .wrap(Wrap { trim: false });
        frame.render_widget(msg, area);
        return;
    }

    if model.agents.is_empty() {
        let msg = Paragraph::new("No agents found.\n\nPress 'n' to create one.")
            .style(Style::default().fg(SUBTLE))
            .block(agents_block);
        frame.render_widget(msg, area);
        return;
    }

    let inner = agents_block.inner(area);
    frame.render_widget(agents_block, area);

    let items: Vec<ListItem> = model
        .agents
        .iter()
        .map(|a| {
            let mut lines = vec![Line::from(format!("{}  ({})", a.name, a.id))];
            if let Some(desc) = &a.description {
                let desc = desc.trim();
                if !desc.is_empty() {
                    lines.push(Line::from(Span::styled(
                        desc.to_string(),
                        Style::default()
                            .fg(SUBTLE)
                            .add_modifier(Modifier::ITALIC),
                    )));
                }
            }
            ListItem::new(lines)
        })
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let content_area = Rect {
        width: inner.width.saturating_sub(1),
        ..inner
    };
    frame.render_stateful_widget(list, content_area, &mut model.agents_list_state);

    let total = model.agents.len();
    let viewport = inner.height as usize;
    if total > viewport {
        let scrollbar_area = Rect {
            x: inner.x + inner.width.saturating_sub(1),
            width: 1,
            ..inner
        };
        let mut scrollbar_state = ScrollbarState::default()
            .content_length(total)
            .viewport_content_length(viewport)
            .position(model.agent_selected_index);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            scrollbar_area,
            &mut scrollbar_state,
        );
    }
}

// ---------------------------------------------------------------------------
// Chats panel
// ---------------------------------------------------------------------------

/// Returns indices into `model.sessions` that match the currently selected agent.
/// If no agent is selected or agents list is empty, returns all indices.
pub fn filtered_session_indices(model: &Model) -> Vec<usize> {
    let selected_agent_id = if !model.agents.is_empty() {
        let idx = model.agent_selected_index.min(model.agents.len() - 1);
        Some(&model.agents[idx].id)
    } else {
        None
    };

    model
        .sessions
        .iter()
        .enumerate()
        .filter(|(_, s)| match selected_agent_id {
            Some(aid) => s.agent_id == *aid,
            None => true,
        })
        .map(|(i, _)| i)
        .collect()
}

fn render_chats_panel(frame: &mut Frame, model: &mut Model, area: Rect) {
    let style = panel_style(model.active, ActivePanel::Chats);

    let filtered = filtered_session_indices(model);
    let is_filtered = !model.agents.is_empty();

    let title = if model.conversations_loading {
        " Chats (loading...) [↑↓] [Enter] [x close] ".to_string()
    } else if is_filtered {
        format!(
            " Chats ({}/{}) [↑↓] [Enter] [x close] ",
            filtered.len(),
            model.sessions.len()
        )
    } else {
        format!(" Chats ({}) [↑↓] [Enter] [x close] ", model.sessions.len())
    };

    let chats_block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(style);

    if filtered.is_empty() {
        let empty_msg = if model.conversations_loading {
            "Loading conversations..."
        } else if model.sessions.is_empty() {
            "No active chats.\n\nSelect an agent and press Enter."
        } else {
            "No chats for this agent.\n\nPress Enter in Agents to start one."
        };
        let msg = Paragraph::new(empty_msg)
            .style(Style::default().fg(SUBTLE))
            .block(chats_block);
        frame.render_widget(msg, area);
        return;
    }

    let inner = chats_block.inner(area);
    frame.render_widget(chats_block, area);

    let highlight_within_filtered = model
        .active_session_index
        .and_then(|active_idx| filtered.iter().position(|&fi| fi == active_idx));

    let items: Vec<ListItem> = filtered
        .iter()
        .map(|&i| {
            let s = &model.sessions[i];
            let indicator = if model.active_session_index == Some(i) {
                "●"
            } else {
                "○"
            };
            let display_title = truncate_title(&s.title, 20);
            let status = if s.history_loading {
                " (loading...)".to_string()
            } else if s.from_server && !s.history_loaded {
                " (saved)".to_string()
            } else {
                String::new()
            };
            let line = Line::from(vec![
                Span::styled(
                    format!("{indicator} "),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(display_title),
                Span::styled(
                    format!(" <{}>", s.agent_name),
                    Style::default().fg(SUBTLE),
                ),
                Span::styled(status, Style::default().fg(SUBTLE)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let mut list_state = ListState::default();
    list_state.select(highlight_within_filtered);
    frame.render_stateful_widget(list, inner, &mut list_state);
}

// ---------------------------------------------------------------------------
// Center panel — Chat
// ---------------------------------------------------------------------------

fn render_center_panel(frame: &mut Frame, model: &mut Model, area: Rect) {
    let is_focused = model.active == ActivePanel::Chat;
    let style = panel_style(model.active, ActivePanel::Chat);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(5)])
        .split(area);

    render_chat_history(frame, model, style, rows[0]);
    render_chat_input(frame, model, style, is_focused, rows[1]);
}

fn render_chat_history(frame: &mut Frame, model: &mut Model, style: Style, area: Rect) {
    let history_block = Block::default()
        .title(" Chat [↑↓ PgUp/PgDn scroll] [Ctrl+R refresh] ")
        .borders(Borders::ALL)
        .border_style(style);

    let inner = history_block.inner(area);
    let content_w = inner.width.saturating_sub(1); // leave room for scrollbar

    let mut lines: Vec<Line<'static>> = Vec::new();
    let bubble_border = Style::default().fg(SUBTLE);

    if let Some(session) = model.active_session() {
        if session.history_loading {
            lines.push(Line::styled(
                "Loading conversation history...",
                Style::default().fg(Color::Yellow),
            ));
        } else if session.from_server && !session.history_loaded && session.chat.is_empty() {
            lines.push(Line::styled(
                "Press Enter in the Chats panel to load this conversation.",
                Style::default().fg(SUBTLE),
            ));
        } else {
            for entry in &session.chat {
                match entry.role {
                    ChatRole::User => {
                        let label = Line::from(vec![
                            Span::styled(
                                "[you]".to_string(),
                                Style::default()
                                    .fg(Color::Green)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw(" "), // keep clear of scrollbar
                        ])
                        .right_aligned();
                        lines.push(label);
                        lines.extend(bubble_lines(
                            &entry.content,
                            content_w,
                            bubble_border,
                            true,
                        ));
                        lines.push(Line::from(""));
                    }
                    ChatRole::Assistant => {
                        let label = Line::from(Span::styled(
                            "[agent]".to_string(),
                            Style::default()
                                .fg(Color::Magenta)
                                .add_modifier(Modifier::BOLD),
                        ));
                        lines.push(label);
                        lines.extend(bubble_lines(
                            &entry.content,
                            content_w,
                            bubble_border,
                            false,
                        ));
                        lines.push(Line::from(""));
                    }
                    ChatRole::System => {
                        lines.push(
                            Line::from(Span::styled(
                                format!(">> {}", entry.content),
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::ITALIC),
                            ))
                            .alignment(Alignment::Center),
                        );
                        lines.push(Line::from(""));
                    }
                }
            }
        }
    }

    if lines.is_empty() {
        lines.push(Line::styled(
            "Select an agent and press Enter to start a chat...",
            Style::default().fg(SUBTLE),
        ));
    }

    let viewport_h = inner.height as usize;
    // Bubble lines are pre-wrapped, so each Line is exactly 1 visual line.
    let total_lines = lines.len();
    let max_scroll_from_top = total_lines.saturating_sub(viewport_h);
    let max_scroll_from_top_u16 = max_scroll_from_top.min(u16::MAX as usize) as u16;

    let from_bottom = model
        .active_session()
        .map(|s| s.chat_scroll_from_bottom.min(max_scroll_from_top_u16))
        .unwrap_or(0);
    let scroll_from_top = max_scroll_from_top_u16.saturating_sub(from_bottom);

    if let Some(session) = model.active_session_mut() {
        session.chat_scroll_from_bottom = from_bottom;
    }

    let history = Paragraph::new(lines)
        .block(history_block)
        .scroll((scroll_from_top, 0));

    frame.render_widget(history, area);

    if inner.width >= 2 && inner.height > 0 && total_lines > viewport_h {
        let sb_area = Rect {
            x: inner.x + inner.width.saturating_sub(1),
            y: inner.y,
            width: 1,
            height: inner.height,
        };

        let mut sb_state = ScrollbarState::new(total_lines)
            .position(scroll_from_top as usize)
            .viewport_content_length(viewport_h);

        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            sb_area,
            &mut sb_state,
        );
    }
}

fn render_chat_input(frame: &mut Frame, model: &Model, style: Style, is_focused: bool, area: Rect) {
    let session = model.active_session();

    let waiting = session.is_some_and(|s| s.waiting_for_response);
    let agent_label = session.map(|s| s.agent_name.as_str()).unwrap_or("none");
    let model_label = "Agent Builder";

    let title = if waiting {
        format!(" Waiting... | {agent_label} | {model_label} ")
    } else {
        format!(" Input | {agent_label} | {model_label} ")
    };

    let input_block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(style);

    let inner = input_block.inner(area);

    let buf = session.map(|s| s.input_buffer.as_str()).unwrap_or("");
    let input_widget = Paragraph::new(buf)
        .block(input_block)
        .wrap(Wrap { trim: false });

    frame.render_widget(input_widget, area);

    if is_focused
        && let Some(session) = session
    {
        let cursor_col =
            cursor_column_in_area(&session.input_buffer, session.input_cursor, inner.width);
        let cursor_row =
            cursor_row_in_area(&session.input_buffer, session.input_cursor, inner.width);

        frame.set_cursor_position(Position::new(inner.x + cursor_col, inner.y + cursor_row));
    }
}

fn cursor_column_in_area(s: &str, byte_cursor: usize, width: u16) -> u16 {
    if width == 0 {
        return 0;
    }
    let prefix = &s[..byte_cursor.min(s.len())];
    let char_count = prefix.chars().count() as u16;
    char_count % width
}

fn cursor_row_in_area(s: &str, byte_cursor: usize, width: u16) -> u16 {
    if width == 0 {
        return 0;
    }
    let prefix = &s[..byte_cursor.min(s.len())];
    let char_count = prefix.chars().count() as u16;
    char_count / width
}

fn truncate_title(title: &str, max_chars: usize) -> String {
    let trimmed = title.trim();
    if trimmed.chars().count() <= max_chars {
        trimmed.to_string()
    } else {
        let truncated: String = trimmed.chars().take(max_chars).collect();
        format!("{truncated}...")
    }
}

// ---------------------------------------------------------------------------
// Chat bubble rendering
// ---------------------------------------------------------------------------

fn bubble_lines(
    content: &str,
    width: u16,
    border_style: Style,
    align_right: bool,
) -> Vec<Line<'static>> {
    let total_w = width as usize;
    if total_w < 6 {
        return wrap_text(content, total_w.max(1))
            .into_iter()
            .map(|s| Line::from(Span::raw(s)))
            .collect();
    }

    let margin = 1usize;
    let max_bubble_w = total_w.saturating_sub(margin).max(4);
    let max_inner_w = max_bubble_w.saturating_sub(2).max(1);

    let mut wrapped: Vec<String> = Vec::new();
    for line in content.lines() {
        if line.is_empty() {
            wrapped.push(String::new());
        } else {
            wrapped.extend(wrap_text(line, max_inner_w));
        }
    }
    if content.ends_with('\n') {
        wrapped.push(String::new());
    }
    if wrapped.is_empty() {
        wrapped.push(String::new());
    }

    let longest = wrapped.iter().map(|s| s.chars().count()).max().unwrap_or(1);
    let inner_w = longest.clamp(1, max_inner_w);
    let bubble_w = inner_w + 2;

    let left_pad = if align_right {
        total_w.saturating_sub(bubble_w + margin)
    } else {
        margin
    };
    let pad = " ".repeat(left_pad);

    let mut out: Vec<Line<'static>> = Vec::new();

    out.push(Line::from(vec![
        Span::raw(pad.clone()),
        Span::styled("┌".to_string(), border_style),
        Span::styled("─".repeat(inner_w), border_style),
        Span::styled("┐".to_string(), border_style),
    ]));

    for line in wrapped {
        let w = line.chars().count();
        let mut body = line;
        if w < inner_w {
            body.push_str(&" ".repeat(inner_w - w));
        }
        out.push(Line::from(vec![
            Span::raw(pad.clone()),
            Span::styled("│".to_string(), border_style),
            Span::raw(body),
            Span::styled("│".to_string(), border_style),
        ]));
    }

    out.push(Line::from(vec![
        Span::raw(pad),
        Span::styled("└".to_string(), border_style),
        Span::styled("─".repeat(inner_w), border_style),
        Span::styled("┘".to_string(), border_style),
    ]));

    out
}

fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut remaining = text;
    while remaining.chars().count() > max_width {
        let break_at: usize = remaining.chars().take(max_width).map(|c| c.len_utf8()).sum();
        lines.push(remaining[..break_at].to_string());
        remaining = &remaining[break_at..];
    }
    lines.push(remaining.to_string());
    lines
}

// ---------------------------------------------------------------------------
// Components panel
// ---------------------------------------------------------------------------

fn render_components_panel(frame: &mut Frame, model: &Model, area: Rect) {
    let style = panel_style(model.active, ActivePanel::Components);

    let outer_block = Block::default()
        .title(" Components [◀-▶ switch tab] [i import] ")
        .borders(Borders::ALL)
        .border_style(style);

    let inner = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    if inner.height < 3 || inner.width < 4 {
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(inner);

    let tabs_area = layout[0];
    let content_area = layout[1];

    let tabs = Tabs::new(vec![
        Line::from("◀ Plugins"),
        Line::from("Skills"),
        Line::from("Tools ▶"),
    ])
    .select(match model.components_tab {
        ComponentsTab::Plugins => 0,
        ComponentsTab::Skills => 1,
        ComponentsTab::Tools => 2,
    })
    .highlight_style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(tabs, tabs_area);

    match model.components_tab {
        ComponentsTab::Plugins => {
            render_components_list(
                frame,
                content_area,
                model.components_plugins_loading,
                model.components_plugins_error.as_deref(),
                "plugins",
                model
                    .components_plugins
                    .iter()
                    .map(|p| {
                        let version = if p.version.is_empty() {
                            String::new()
                        } else {
                            format!(" v{}", p.version)
                        };
                        let skills = if p.skill_ids.is_empty() {
                            String::new()
                        } else {
                            format!(" ({} skills)", p.skill_ids.len())
                        };
                        (
                            p.name.clone(),
                            format!("{}{version}{skills}", p.description),
                            p.readonly,
                        )
                    })
                    .collect(),
            );
        }
        ComponentsTab::Skills => {
            render_components_list(
                frame,
                content_area,
                model.components_skills_loading,
                model.components_skills_error.as_deref(),
                "skills",
                model
                    .components_skills
                    .iter()
                    .map(|s| {
                        let plugin_tag = if s.plugin_id.is_some() {
                            " (plugin)"
                        } else {
                            ""
                        };
                        (
                            s.name.clone(),
                            format!("{}{plugin_tag}", s.description),
                            s.readonly,
                        )
                    })
                    .collect(),
            );
        }
        ComponentsTab::Tools => {
            render_components_list(
                frame,
                content_area,
                model.components_tools_loading,
                model.components_tools_error.as_deref(),
                "tools",
                model
                    .components_tools
                    .iter()
                    .map(|t| (t.id.clone(), t.description.clone(), t.readonly))
                    .collect(),
            );
        }
    }
}

fn render_components_list(
    frame: &mut Frame,
    area: Rect,
    loading: bool,
    error: Option<&str>,
    label: &str,
    items: Vec<(String, String, bool)>,
) {
    if loading {
        let msg = Paragraph::new(format!("Loading {label}..."))
            .style(Style::default().fg(SUBTLE));
        frame.render_widget(msg, area);
        return;
    }
    if let Some(err) = error {
        let msg = Paragraph::new(format!("Error: {err}"))
            .style(Style::default().fg(Color::Red))
            .wrap(Wrap { trim: false });
        frame.render_widget(msg, area);
        return;
    }
    if items.is_empty() {
        let msg = Paragraph::new(format!("No {label} available."))
            .style(Style::default().fg(SUBTLE));
        frame.render_widget(msg, area);
        return;
    }

    let list_items: Vec<ListItem> = items
        .iter()
        .map(|(name, desc, readonly)| {
            let tag = if *readonly { " (built-in)" } else { "" };
            let line = Line::from(vec![
                Span::styled("• ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("{name} ")),
                Span::styled(
                    format!("{desc}{tag}"),
                    Style::default().fg(SUBTLE),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let count = list_items.len();
    let list = List::new(list_items);
    let title_line = Line::from(Span::styled(
        format!(" {count} {label} "),
        Style::default().fg(SUBTLE),
    ));
    let header = Paragraph::new(title_line);

    if area.height < 2 {
        frame.render_widget(list, area);
        return;
    }

    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    frame.render_widget(header, split[0]);
    frame.render_widget(list, split[1]);
}

// ---------------------------------------------------------------------------
// Modal overlay
// ---------------------------------------------------------------------------

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vert[1])[1]
}

fn render_modal(frame: &mut Frame, modal: &mut Modal) {
    match modal {
        Modal::MissingEnv { missing } => {
            let rect = centered_rect(50, 30, frame.area());
            frame.render_widget(Clear, rect);
            let msg = format!(
                "Missing configuration:\n\n{}\n\nPress Enter to dismiss.",
                missing.join(", ")
            );
            let widget = Paragraph::new(msg)
                .block(
                    Block::default()
                        .title(" Missing Environment ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Yellow)),
                )
                .wrap(Wrap { trim: false });
            frame.render_widget(widget, rect);
        }

        Modal::Info { title, message } => {
            render_simple_modal(frame, title, message, Color::Cyan);
        }

        Modal::Error { title, message } => {
            render_simple_modal(frame, title, message, Color::Red);
        }

        Modal::ConfirmDeleteAgent(state) => {
            let rect = centered_rect(50, 25, frame.area());
            frame.render_widget(Clear, rect);
            let msg = if state.deleting {
                format!("Deleting {}...", state.agent_name)
            } else {
                format!(
                    "Delete agent \"{}\"?\n\n[y] Yes  [n/Esc] Cancel",
                    state.agent_name
                )
            };
            let widget = Paragraph::new(msg)
                .block(
                    Block::default()
                        .title(" Confirm Delete ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Red)),
                )
                .wrap(Wrap { trim: false });
            frame.render_widget(widget, rect);
        }

        Modal::CreateAgent(state) => {
            render_create_agent_modal(frame, state);
        }

        Modal::Import(state) => {
            render_import_modal(frame, state);
        }
    }
}

fn render_simple_modal(frame: &mut Frame, title: &str, message: &str, color: Color) {
    let rect = centered_rect(50, 30, frame.area());
    frame.render_widget(Clear, rect);
    let msg = format!("{message}\n\nPress Enter to dismiss.");
    let widget = Paragraph::new(msg)
        .block(
            Block::default()
                .title(format!(" {title} "))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, rect);
}

fn render_import_modal(frame: &mut Frame, state: &ImportModal) {
    let rect = centered_rect(60, 70, frame.area());
    frame.render_widget(Clear, rect);

    let type_label = match state.component_type {
        ComponentsTab::Plugins => "Plugin",
        ComponentsTab::Skills => "Skill",
        ComponentsTab::Tools => "Tool",
    };

    let outer_block = Block::default()
        .title(format!(" Import {type_label} from YAML "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = outer_block.inner(rect);
    frame.render_widget(outer_block, rect);

    if inner.height < 4 || inner.width < 10 {
        return;
    }

    let has_error = state.error_message.is_some();
    let footer_height = if has_error { 3 } else { 2 };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(footer_height),
        ])
        .split(inner);

    let explorer_area = layout[0];
    let footer_area = layout[1];

    frame.render_widget_ref(state.file_explorer.widget(), explorer_area);

    let mut footer_lines = Vec::new();
    if let Some(err) = &state.error_message {
        footer_lines.push(Line::styled(
            err.as_str(),
            Style::default().fg(Color::Red),
        ));
    }
    footer_lines.push(Line::styled(
        "[Enter] Select  [Esc] Cancel  [↑↓] Navigate  [←/Backspace] Up  [→/Enter] Open dir",
        Style::default().fg(SUBTLE),
    ));

    let footer = Paragraph::new(footer_lines).wrap(Wrap { trim: false });
    frame.render_widget(footer, footer_area);
}

fn render_create_agent_modal(frame: &mut Frame, state: &mut CreateAgentModal) {
    let rect = centered_rect(70, 70, frame.area());
    frame.render_widget(Clear, rect);

    let title = match &state.mode {
        AgentEditorMode::Create => " Create Agent ",
        AgentEditorMode::Edit { .. } => " Edit Agent ",
    };

    let outer_block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = outer_block.inner(rect);
    frame.render_widget(outer_block, rect);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(4),
            Constraint::Length(3),
        ])
        .split(inner);

    let tabs_area = layout[0];
    let content_area = layout[1];
    let help_area = layout[2];

    let tabs = Tabs::new(vec![
        Line::from("◀ Prompt"),
        Line::from(format!("Tools ({})", state.selected_tool_ids.len())),
        Line::from(format!("Skills ({})", state.selected_skill_ids.len())),
        Line::from(format!("Plugins ({}) ▶", state.selected_plugin_ids.len())),
    ])
    .select(match state.tab {
        CreateAgentTab::Prompt => 0,
        CreateAgentTab::Tools => 1,
        CreateAgentTab::Skills => 2,
        CreateAgentTab::Plugins => 3,
    })
    .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    frame.render_widget(tabs, tabs_area);

    match state.tab {
        CreateAgentTab::Prompt => render_prompt_tab(frame, state, content_area),
        CreateAgentTab::Tools => render_tools_tab(frame, state, content_area),
        CreateAgentTab::Skills => render_skills_tab(frame, state, content_area),
        CreateAgentTab::Plugins => render_plugins_tab(frame, state, content_area),
    }

    let mut help_lines = vec![Line::from(
        "Tab: next field | ◀-▶: switch tab | Ctrl+S: save | Esc: cancel",
    )];
    if state.submitting {
        help_lines.push(Line::styled(
            "Saving...",
            Style::default().fg(Color::Yellow),
        ));
    }
    if let Some(err) = &state.error {
        help_lines.push(Line::styled(
            err.as_str(),
            Style::default().fg(Color::Red),
        ));
    }
    let help = Paragraph::new(help_lines)
        .style(Style::default().fg(SUBTLE))
        .wrap(Wrap { trim: false });
    frame.render_widget(help, help_area);
}

fn render_prompt_tab(frame: &mut Frame, state: &CreateAgentModal, area: Rect) {
    let fields = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(3),
        ])
        .split(area);

    let focus_style = Style::default().fg(Color::Cyan);
    let normal_style = Style::default().fg(BORDER_NORMAL);

    let name_style = if state.focus == CreateAgentField::Name {
        focus_style
    } else {
        normal_style
    };
    let caret = if state.focus == CreateAgentField::Name {
        "▍"
    } else {
        ""
    };
    let name_widget = Paragraph::new(format!("{}{caret}", state.name)).block(
        Block::default()
            .title(" Name ")
            .borders(Borders::ALL)
            .border_style(name_style),
    );
    frame.render_widget(name_widget, fields[0]);

    let desc_style = if state.focus == CreateAgentField::Description {
        focus_style
    } else {
        normal_style
    };
    let caret = if state.focus == CreateAgentField::Description {
        "▍"
    } else {
        ""
    };
    let desc_widget = Paragraph::new(format!("{}{caret}", state.description)).block(
        Block::default()
            .title(" Description ")
            .borders(Borders::ALL)
            .border_style(desc_style),
    );
    frame.render_widget(desc_widget, fields[1]);

    let instr_style = if state.focus == CreateAgentField::Instructions {
        focus_style
    } else {
        normal_style
    };
    let caret = if state.focus == CreateAgentField::Instructions {
        "▍"
    } else {
        ""
    };
    let instr_widget = Paragraph::new(format!("{}{caret}", state.instructions))
        .block(
            Block::default()
                .title(" Instructions ")
                .borders(Borders::ALL)
                .border_style(instr_style),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(instr_widget, fields[2]);
}

fn render_tools_tab(frame: &mut Frame, state: &mut CreateAgentModal, area: Rect) {
    if state.tools_loading {
        let msg = Paragraph::new("Loading tools...").style(Style::default().fg(SUBTLE));
        frame.render_widget(msg, area);
        return;
    }
    if let Some(err) = &state.tools_error {
        let msg = Paragraph::new(format!("Error: {err}"))
            .style(Style::default().fg(Color::Red))
            .wrap(Wrap { trim: false });
        frame.render_widget(msg, area);
        return;
    }
    if state.tools.is_empty() {
        let msg = Paragraph::new("No tools available.").style(Style::default().fg(SUBTLE));
        frame.render_widget(msg, area);
        return;
    }

    let items: Vec<ListItem> = state
        .tools
        .iter()
        .map(|t| {
            let checked = if state.selected_tool_ids.contains(&t.id) {
                "[x]"
            } else {
                "[ ]"
            };
            let line = Line::from(vec![
                Span::styled(format!("{checked} "), Style::default().fg(Color::Cyan)),
                Span::raw(&t.id),
                Span::styled(
                    format!("  {}", t.description),
                    Style::default().fg(SUBTLE),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, &mut state.tools_list_state);
}

fn render_skills_tab(frame: &mut Frame, state: &mut CreateAgentModal, area: Rect) {
    if state.skills_loading {
        let msg = Paragraph::new("Loading skills...").style(Style::default().fg(SUBTLE));
        frame.render_widget(msg, area);
        return;
    }
    if let Some(err) = &state.skills_error {
        let msg = Paragraph::new(format!("Error: {err}"))
            .style(Style::default().fg(Color::Red))
            .wrap(Wrap { trim: false });
        frame.render_widget(msg, area);
        return;
    }
    if state.skills.is_empty() {
        let msg = Paragraph::new("No skills available.").style(Style::default().fg(SUBTLE));
        frame.render_widget(msg, area);
        return;
    }

    let items: Vec<ListItem> = state
        .skills
        .iter()
        .map(|s| {
            let checked = if state.selected_skill_ids.contains(&s.id) {
                "[x]"
            } else {
                "[ ]"
            };
            let plugin_tag = s
                .plugin_id
                .as_deref()
                .map(|_| " (plugin)")
                .unwrap_or("");
            let readonly_tag = if s.readonly { " (built-in)" } else { "" };
            let line = Line::from(vec![
                Span::styled(format!("{checked} "), Style::default().fg(Color::Cyan)),
                Span::raw(format!("{} ", s.name)),
                Span::styled(
                    format!("{}{readonly_tag}{plugin_tag}", s.description),
                    Style::default().fg(SUBTLE),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, &mut state.skills_list_state);
}

fn render_plugins_tab(frame: &mut Frame, state: &mut CreateAgentModal, area: Rect) {
    if state.plugins_loading {
        let msg = Paragraph::new("Loading plugins...").style(Style::default().fg(SUBTLE));
        frame.render_widget(msg, area);
        return;
    }
    if let Some(err) = &state.plugins_error {
        let msg = Paragraph::new(format!("Error: {err}"))
            .style(Style::default().fg(Color::Red))
            .wrap(Wrap { trim: false });
        frame.render_widget(msg, area);
        return;
    }
    if state.plugins.is_empty() {
        let msg = Paragraph::new("No plugins available.").style(Style::default().fg(SUBTLE));
        frame.render_widget(msg, area);
        return;
    }

    let items: Vec<ListItem> = state
        .plugins
        .iter()
        .map(|p| {
            let checked = if state.selected_plugin_ids.contains(&p.id) {
                "[x]"
            } else {
                "[ ]"
            };
            let skills_info = if p.skill_ids.is_empty() {
                String::new()
            } else {
                format!(" ({} skills)", p.skill_ids.len())
            };
            let version_tag = if p.version.is_empty() {
                String::new()
            } else {
                format!(" v{}", p.version)
            };
            let line = Line::from(vec![
                Span::styled(format!("{checked} "), Style::default().fg(Color::Cyan)),
                Span::raw(format!("{} ", p.name)),
                Span::styled(
                    format!("{}{version_tag}{skills_info}", p.description),
                    Style::default().fg(SUBTLE),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, &mut state.plugins_list_state);
}

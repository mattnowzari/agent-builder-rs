use ratatui::Frame;
use unicode_width::UnicodeWidthStr;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, FrameExt as _, List, ListItem, ListState, Paragraph, Scrollbar,
    ScrollbarOrientation, ScrollbarState, Tabs, Wrap,
};

use super::model::{
    ActivePanel, AgentEditorMode, ChatRole, ComponentsTab, ConfirmDeleteComponentModal,
    CreateAgentField, CreateAgentModal, CreateAgentTab, GitHubImportModal, ImportChooserModal,
    ImportModal, ImportTarget, InstallPluginModal, Modal, Model, ConfirmDeleteConversationModal,
};
use crate::theme::Theme;

fn panel_style(theme: &Theme, active: ActivePanel, this: ActivePanel) -> Style {
    if active == this {
        Style::default().fg(theme.border_focused)
    } else {
        Style::default().fg(theme.border_normal)
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

    let theme = model.theme.clone();
    if let Some(modal) = model.modal.as_mut() {
        render_modal(frame, modal, &theme);
    }
}

// ---------------------------------------------------------------------------
// Agents panel
// ---------------------------------------------------------------------------

fn render_agents_panel(frame: &mut Frame, model: &mut Model, area: Rect) {
    let t = &model.theme;
    let style = panel_style(t, model.active, ActivePanel::Agents);

    let agents_block = Block::default()
        .title(" Agents [↑↓] [Enter chat] [Ctrl+R refresh] ")
        .title_bottom(" [n new] [e edit] [d del] [i import] ")
        .borders(Borders::ALL)
        .border_style(style);

    if !model.env_loaded || model.agents_loading {
        let msg = Paragraph::new("Loading agents...")
            .style(Style::default().fg(t.text_subtle))
            .block(agents_block);
        frame.render_widget(msg, area);
        return;
    }

    if let Some(err) = &model.agents_error {
        let msg = Paragraph::new(format!("Error: {err}\n\nCtrl+R to retry"))
            .style(Style::default().fg(t.text_error))
            .block(agents_block)
            .wrap(Wrap { trim: false });
        frame.render_widget(msg, area);
        return;
    }

    if model.agents.is_empty() {
        let msg = Paragraph::new("No agents found.\n\nPress 'n' to create one.")
            .style(Style::default().fg(t.text_subtle))
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
                            .fg(t.text_subtle)
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
                .bg(t.highlight_bg)
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
    let t = &model.theme;
    let style = panel_style(t, model.active, ActivePanel::Chats);

    let filtered = filtered_session_indices(model);
    let is_filtered = !model.agents.is_empty();

    let title = if model.conversations_loading {
        " Chats (loading...) ".to_string()
    } else if is_filtered {
        format!(" Chats ({}) ", filtered.len())
    } else {
        format!(" Chats ({}) ", model.sessions.len())
    };

    let chats_block = Block::default()
        .title(title)
        .title_bottom(" [Enter open] [d delete] [Ctrl+R refresh] ")
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
            .style(Style::default().fg(t.text_subtle))
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
                    Style::default().fg(t.text_primary),
                ),
                Span::raw(display_title),
                Span::styled(
                    format!(" <{}>", s.agent_name),
                    Style::default().fg(t.text_subtle),
                ),
                Span::styled(status, Style::default().fg(t.text_subtle)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(t.highlight_bg)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let content_area = Rect {
        width: inner.width.saturating_sub(1),
        ..inner
    };
    let mut list_state = ListState::default();
    list_state.select(highlight_within_filtered);
    frame.render_stateful_widget(list, content_area, &mut list_state);

    let total = filtered.len();
    let viewport = inner.height as usize;
    if total > viewport {
        let scrollbar_area = Rect {
            x: inner.x + inner.width.saturating_sub(1),
            width: 1,
            ..inner
        };
        let position = highlight_within_filtered.unwrap_or(0);
        let mut scrollbar_state = ScrollbarState::default()
            .content_length(total)
            .viewport_content_length(viewport)
            .position(position);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            scrollbar_area,
            &mut scrollbar_state,
        );
    }
}

// ---------------------------------------------------------------------------
// Center panel — Chat
// ---------------------------------------------------------------------------

fn render_center_panel(frame: &mut Frame, model: &mut Model, area: Rect) {
    let is_focused = model.active == ActivePanel::Chat;
    let style = panel_style(&model.theme, model.active, ActivePanel::Chat);

    let border_overhead: u16 = 2;
    let inner_width = area.width.saturating_sub(border_overhead) as usize;

    let input_chars = model
        .active_session()
        .map(|s| s.input_buffer.chars().count())
        .unwrap_or(0);

    let wrapped_lines = if inner_width > 0 {
        (input_chars / inner_width) as u16 + 1
    } else {
        1
    };
    let input_height = wrapped_lines.clamp(3, 10) + border_overhead;

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(input_height)])
        .split(area);

    render_chat_history(frame, model, style, rows[0]);
    render_chat_input(frame, model, style, is_focused, rows[1]);
}

fn render_chat_history(frame: &mut Frame, model: &mut Model, style: Style, area: Rect) {
    let history_block = Block::default()
        .title(" Chat ")
        .title_bottom(" [↑↓ PgUp/PgDn scroll] [Ctrl+R refresh] ")
        .borders(Borders::ALL)
        .border_style(style);

    let inner = history_block.inner(area);
    let content_w = inner.width.saturating_sub(1); // leave room for scrollbar

    let t = &model.theme;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let bubble_border = Style::default().fg(t.text_subtle);

    if let Some(session) = model.active_session() {
        if session.history_loading {
            lines.push(Line::styled(
                "Loading conversation history...",
                Style::default().fg(t.text_warning),
            ));
        } else if session.from_server && !session.history_loaded && session.chat.is_empty() {
            lines.push(Line::styled(
                "Press Enter in the Chats panel to load this conversation.",
                Style::default().fg(t.text_subtle),
            ));
        } else {
            let theme_snapshot = t.clone();
            for entry in &session.chat {
                match entry.role {
                    ChatRole::User => {
                        let label = Line::from(vec![
                            Span::styled(
                                "[you]".to_string(),
                                Style::default()
                                    .fg(theme_snapshot.text_user)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw(" "),
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
                        if !entry.steps.is_empty() {
                            lines.extend(thought_process_lines(&entry.steps, content_w, &theme_snapshot));
                        }
                        let label = Line::from(Span::styled(
                            "[agent]".to_string(),
                            Style::default()
                                .fg(theme_snapshot.text_agent)
                                .add_modifier(Modifier::BOLD),
                        ));
                        lines.push(label);
                        lines.extend(markdown_bubble_lines(
                            &entry.content,
                            content_w,
                            bubble_border,
                            &theme_snapshot,
                        ));
                        lines.push(Line::from(""));
                    }
                    ChatRole::System => {
                        lines.push(
                            Line::from(Span::styled(
                                format!(">> {}", entry.content),
                                Style::default()
                                    .fg(theme_snapshot.text_warning)
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
            Style::default().fg(t.text_subtle),
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
    let model_label = session
        .and_then(|s| s.model_name.as_deref())
        .unwrap_or("model unknown");

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
    // Pre-wrap using the same char-count algorithm as cursor_column/row_in_area so
    // that the rendered line breaks match the cursor position calculations exactly.
    // Ratatui's Wrap widget breaks at word boundaries, which diverges from the
    // simple `char_count % width` math and causes the cursor to appear in the wrong
    // column/row when the text contains spaces.
    let lines = char_wrap_input(buf, inner.width as usize);
    let input_widget = Paragraph::new(lines).block(input_block);

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

/// Splits `s` into display lines of exactly `width` characters each.
/// This matches the char-count arithmetic used by `cursor_column_in_area` and
/// `cursor_row_in_area`, ensuring rendered line breaks and cursor position agree.
fn char_wrap_input(s: &str, width: usize) -> Vec<Line<'static>> {
    if width == 0 {
        return vec![Line::from(s.to_string())];
    }
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return vec![Line::from(String::new())];
    }
    let mut lines = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let end = (start + width).min(chars.len());
        lines.push(Line::from(chars[start..end].iter().collect::<String>()));
        start = end;
    }
    lines
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
    if UnicodeWidthStr::width(trimmed) <= max_chars {
        trimmed.to_string()
    } else {
        let end = split_at_width(trimmed, max_chars);
        format!("{}...", &trimmed[..end])
    }
}

// ---------------------------------------------------------------------------
// Thought process (tool-call steps) rendering
// ---------------------------------------------------------------------------

use crate::agent_builder::ToolStep;

fn thought_process_lines(steps: &[ToolStep], width: u16, theme: &Theme) -> Vec<Line<'static>> {
    let w = width as usize;
    let mut out: Vec<Line<'static>> = Vec::new();

    let tool_calls = steps.iter().filter(|s| !s.tool_id.is_empty()).count();
    let label = if tool_calls > 0 {
        format!(" ╭─ Thought Process ({tool_calls} tool call{}) ", if tool_calls == 1 { "" } else { "s" })
    } else {
        " ╭─ Thought Process ".to_string()
    };
    out.push(Line::from(Span::styled(
        label,
        Style::default().fg(theme.thought_dim).add_modifier(Modifier::ITALIC),
    )));

    for (i, step) in steps.iter().enumerate() {
        let is_last = i + 1 == steps.len();
        let connector = if is_last { " ╰" } else { " │" };
        let continuation = if is_last { "  " } else { " │" };

        if let Some(reasoning) = &step.reasoning {
            let max_chars = w.saturating_sub(7);
            let display = truncate_str(reasoning, max_chars);
            out.push(Line::from(vec![
                Span::styled(format!("{connector} "), Style::default().fg(theme.thought_dim)),
                Span::styled("💭 ", Style::default().fg(theme.thought_reasoning)),
                Span::styled(display, Style::default().fg(theme.thought_dim).add_modifier(Modifier::ITALIC)),
            ]));
        } else {
            out.push(Line::from(vec![
                Span::styled(format!("{connector} "), Style::default().fg(theme.thought_dim)),
                Span::styled("⚙ ", Style::default().fg(theme.thought_tool)),
                Span::styled(
                    step.tool_id.clone(),
                    Style::default().fg(theme.thought_tool).add_modifier(Modifier::BOLD),
                ),
            ]));

            if !step.params_summary.is_empty() {
                let max_chars = w.saturating_sub(8);
                let display = truncate_str(&step.params_summary, max_chars);
                out.push(Line::from(vec![
                    Span::styled(format!("{continuation}   "), Style::default().fg(theme.thought_dim)),
                    Span::styled("→ ", Style::default().fg(theme.thought_dim)),
                    Span::styled(display, Style::default().fg(theme.thought_dim)),
                ]));
            }

            if !step.result_summary.is_empty() {
                let max_chars = w.saturating_sub(8);
                let display = truncate_str(&step.result_summary, max_chars);
                out.push(Line::from(vec![
                    Span::styled(format!("{continuation}   "), Style::default().fg(theme.thought_dim)),
                    Span::styled("← ", Style::default().fg(theme.thought_result)),
                    Span::styled(display, Style::default().fg(theme.thought_dim)),
                ]));
            }
        }
    }

    out.push(Line::from(""));
    out
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    if max_chars < 4 || UnicodeWidthStr::width(s) <= max_chars {
        return s.to_string();
    }
    let end = split_at_width(s, max_chars - 1); // leave one column for '…'
    format!("{}…", &s[..end])
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

    let longest = wrapped.iter().map(|s| UnicodeWidthStr::width(s.as_str())).max().unwrap_or(1);
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
        let w = UnicodeWidthStr::width(line.as_str());
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

/// Custom StyleSheet that maps tui-markdown styles to our theme.
#[derive(Clone)]
struct ThemeStyleSheet {
    heading_color: Color,
    code_bg: Color,
    text_subtle: Color,
    text_primary: Color,
}

impl tui_markdown::StyleSheet for ThemeStyleSheet {
    fn heading(&self, level: u8) -> Style {
        match level {
            1 => Style::default()
                .fg(self.heading_color)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            2 => Style::default()
                .fg(self.heading_color)
                .add_modifier(Modifier::BOLD),
            3 => Style::default()
                .fg(self.heading_color)
                .add_modifier(Modifier::BOLD | Modifier::ITALIC),
            _ => Style::default()
                .fg(self.heading_color)
                .add_modifier(Modifier::ITALIC),
        }
    }

    fn code(&self) -> Style {
        Style::default().bg(self.code_bg)
    }

    fn link(&self) -> Style {
        Style::default()
            .fg(self.text_primary)
            .add_modifier(Modifier::UNDERLINED)
    }

    fn blockquote(&self) -> Style {
        Style::default().fg(self.text_subtle)
    }

    fn heading_meta(&self) -> Style {
        Style::default().fg(self.text_subtle)
    }

    fn metadata_block(&self) -> Style {
        Style::default().fg(self.text_subtle)
    }
}

/// Strip the leading `# `, `## `, etc. prefix from heading lines produced by
/// tui_markdown, keeping only the text with the line-level heading style.
fn strip_heading_prefix(line: Line<'static>) -> Line<'static> {
    if line.spans.is_empty() {
        return line;
    }
    let first_content = line.spans[0].content.as_ref();
    let is_heading = first_content.starts_with('#')
        && first_content.chars().all(|c| c == '#' || c == ' ');
    if !is_heading {
        return line;
    }
    let style = line.style;
    let alignment = line.alignment;
    let mut new_spans: Vec<Span<'static>> = line
        .spans
        .into_iter()
        .skip(1)
        .map(span_to_owned)
        .collect();
    if new_spans.is_empty() {
        new_spans.push(Span::raw(""));
    }
    let mut new_line = Line::from(new_spans);
    new_line.style = style;
    new_line.alignment = alignment;
    new_line
}

/// Render markdown content inside a left-aligned chat bubble.
/// Uses `tui_markdown` for standard markdown and a custom table renderer for
/// GFM tables (which tui_markdown does not support).
fn markdown_bubble_lines(
    content: &str,
    width: u16,
    border_style: Style,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let total_w = width as usize;
    if total_w < 6 {
        return bubble_lines(content, width, border_style, false);
    }

    let margin = 1usize;
    let max_bubble_w = total_w.saturating_sub(margin).max(4);
    let max_inner_w = max_bubble_w.saturating_sub(2).max(1);

    let style_sheet = ThemeStyleSheet {
        heading_color: theme.text_primary,
        code_bg: theme.highlight_bg,
        text_subtle: theme.text_subtle,
        text_primary: theme.text_primary,
    };
    let options = tui_markdown::Options::new(style_sheet);

    let mut wrapped_lines = render_markdown_lines(content, max_inner_w, &options, theme);

    if wrapped_lines.is_empty() {
        wrapped_lines.push(Line::from(""));
    }

    let longest = wrapped_lines
        .iter()
        .map(|l| line_char_width(l))
        .max()
        .unwrap_or(1);
    let inner_w = longest.clamp(1, max_inner_w);
    let bubble_w = inner_w + 2;

    let left_pad = margin;
    let _ = bubble_w;
    let pad = " ".repeat(left_pad);

    let mut out: Vec<Line<'static>> = Vec::new();

    out.push(Line::from(vec![
        Span::raw(pad.clone()),
        Span::styled("┌".to_string(), border_style),
        Span::styled("─".repeat(inner_w), border_style),
        Span::styled("┐".to_string(), border_style),
    ]));

    for line in wrapped_lines {
        let w = line_char_width(&line);
        let trailing = if w < inner_w {
            " ".repeat(inner_w - w)
        } else {
            String::new()
        };

        let line_style = line.style;
        let mut spans = vec![
            Span::raw(pad.clone()),
            Span::styled("│".to_string(), border_style),
        ];
        for s in line.spans {
            let merged = line_style.patch(s.style);
            spans.push(Span::styled(s.content.into_owned(), merged));
        }
        spans.push(Span::styled(trailing, line_style));
        spans.push(Span::styled("│".to_string(), border_style));

        out.push(Line::from(spans));
    }

    out.push(Line::from(vec![
        Span::raw(pad),
        Span::styled("└".to_string(), border_style),
        Span::styled("─".repeat(inner_w), border_style),
        Span::styled("┘".to_string(), border_style),
    ]));

    out
}

/// Table-aware markdown renderer.
///
/// Uses pulldown-cmark with ENABLE_TABLES to locate GFM tables by byte range,
/// renders them as Unicode box-drawing art, and delegates non-table sections to
/// tui-markdown (which does not support tables itself).
fn render_markdown_lines(
    content: &str,
    max_inner_w: usize,
    options: &tui_markdown::Options<ThemeStyleSheet>,
    theme: &Theme,
) -> Vec<Line<'static>> {
    use pulldown_cmark::{Alignment, Event, Options as CmarkOptions, Parser, Tag, TagEnd};

    struct TableData {
        range: std::ops::Range<usize>,
        alignments: Vec<Alignment>,
        header: Vec<String>,
        rows: Vec<Vec<String>>,
    }

    let mut cmark_opts = CmarkOptions::empty();
    cmark_opts.insert(CmarkOptions::ENABLE_TABLES);

    let mut tables: Vec<TableData> = Vec::new();
    let mut in_table = false;
    let mut table_start = 0usize;
    let mut current_alignments: Vec<Alignment> = Vec::new();
    let mut current_header: Vec<String> = Vec::new();
    let mut current_rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut current_cell = String::new();
    let mut in_header = false;

    for (event, range) in Parser::new_ext(content, cmark_opts).into_offset_iter() {
        match event {
            Event::Start(Tag::Table(aligns)) => {
                in_table = true;
                table_start = range.start;
                current_alignments = aligns;
                current_header.clear();
                current_rows.clear();
            }
            Event::End(TagEnd::Table) => {
                if in_table {
                    tables.push(TableData {
                        range: table_start..range.end,
                        alignments: current_alignments.clone(),
                        header: current_header.clone(),
                        rows: current_rows.clone(),
                    });
                }
                in_table = false;
            }
            Event::Start(Tag::TableHead) => {
                in_header = true;
            }
            Event::End(TagEnd::TableHead) => {
                in_header = false;
            }
            Event::Start(Tag::TableRow) => {
                current_row.clear();
            }
            Event::End(TagEnd::TableRow) => {
                if in_table && !in_header {
                    current_rows.push(std::mem::take(&mut current_row));
                }
            }
            Event::Start(Tag::TableCell) => {
                current_cell.clear();
            }
            Event::End(TagEnd::TableCell) => {
                let cell = std::mem::take(&mut current_cell).trim().to_string();
                if in_table {
                    if in_header {
                        current_header.push(cell);
                    } else {
                        current_row.push(cell);
                    }
                }
            }
            Event::Text(text) if in_table => {
                current_cell.push_str(&text);
            }
            Event::Code(code) if in_table => {
                current_cell.push('`');
                current_cell.push_str(&code);
                current_cell.push('`');
            }
            Event::SoftBreak | Event::HardBreak if in_table => {
                current_cell.push(' ');
            }
            _ => {}
        }
    }

    // Run tui-markdown on a non-table chunk and return wrapped lines.
    let process_chunk = |chunk: &str| -> Vec<Line<'static>> {
        tui_markdown::from_str_with_options(chunk, options)
            .lines
            .into_iter()
            .flat_map(|line| {
                let cleaned = strip_heading_prefix(line_to_owned(line));
                let w = line_char_width(&cleaned);
                if w <= max_inner_w {
                    vec![cleaned]
                } else {
                    wrap_styled_line(cleaned, max_inner_w)
                }
            })
            .collect()
    };

    if tables.is_empty() {
        return process_chunk(content);
    }

    let mut all_lines: Vec<Line<'static>> = Vec::new();
    let mut prev_end = 0usize;

    for table in &tables {
        // Non-table content before this table.
        if prev_end < table.range.start {
            let chunk = content[prev_end..table.range.start].trim_matches('\n');
            if !chunk.is_empty() {
                all_lines.extend(process_chunk(chunk));
                all_lines.push(Line::from(""));
            }
        }

        all_lines.extend(render_table_art_lines(
            &table.alignments,
            &table.header,
            &table.rows,
            max_inner_w,
            theme,
        ));

        prev_end = table.range.end;
    }

    // Non-table content after the last table.
    if prev_end < content.len() {
        let chunk = content[prev_end..].trim_matches('\n');
        if !chunk.is_empty() {
            all_lines.push(Line::from(""));
            all_lines.extend(process_chunk(chunk));
        }
    }

    all_lines
}

/// Render a parsed GFM table as Unicode box-drawing art into ratatui `Line`s.
///
/// Column widths are sized to the widest content in each column, then
/// proportionally shrunk if the total exceeds `max_w`. Cell content that is
/// still too wide is truncated with `…`.
fn render_table_art_lines(
    alignments: &[pulldown_cmark::Alignment],
    header: &[String],
    rows: &[Vec<String>],
    max_w: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    use pulldown_cmark::Alignment;

    let col_count = header
        .len()
        .max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
    if col_count == 0 {
        return Vec::new();
    }

    // Natural column widths: max(header display width, widest data cell), min 1.
    let mut col_widths: Vec<usize> = (0..col_count)
        .map(|i| {
            let h = header.get(i).map(|s| UnicodeWidthStr::width(s.as_str())).unwrap_or(0);
            let d = rows
                .iter()
                .map(|r| r.get(i).map(|s| UnicodeWidthStr::width(s.as_str())).unwrap_or(0))
                .max()
                .unwrap_or(0);
            h.max(d).max(1)
        })
        .collect();

    // Total table width = 1 (left border) + Σ(col_w + 2 padding + 1 right border).
    let table_w: usize = 1 + col_widths.iter().map(|w| w + 3).sum::<usize>();

    if table_w > max_w {
        let available = max_w.saturating_sub(col_count * 3 + 1);
        let total_natural: usize = col_widths.iter().sum();
        if available > 0 && total_natural > 0 {
            let mut new_widths: Vec<usize> = col_widths
                .iter()
                .map(|&w| ((w * available) / total_natural).max(1))
                .collect();
            // Distribute any rounding remainder to the first column.
            let used: usize = new_widths.iter().sum();
            if used < available {
                new_widths[0] += available - used;
            }
            col_widths = new_widths;
        } else {
            col_widths = vec![1; col_count];
        }
    }

    let border_style = Style::default().fg(theme.text_subtle);
    let header_style = Style::default().add_modifier(Modifier::BOLD);

    let trunc = |s: &str, max: usize| -> String {
        if UnicodeWidthStr::width(s) <= max {
            s.to_string()
        } else {
            let end = split_at_width(s, max.saturating_sub(1)); // leave one column for '…'
            format!("{}…", &s[..end])
        }
    };

    let pad_cell = |s: &str, w: usize, align: Alignment| -> String {
        let n = UnicodeWidthStr::width(s);
        let padding = w.saturating_sub(n);
        match align {
            Alignment::Right => format!("{}{}", " ".repeat(padding), s),
            Alignment::Center => {
                let left = padding / 2;
                format!("{}{}{}", " ".repeat(left), s, " ".repeat(padding - left))
            }
            _ => format!("{}{}", s, " ".repeat(padding)),
        }
    };

    let mut out: Vec<Line<'static>> = Vec::new();

    // ┌───┬───┐
    {
        let mut spans = vec![Span::styled("┌".to_string(), border_style)];
        for (i, &w) in col_widths.iter().enumerate() {
            spans.push(Span::styled("─".repeat(w + 2), border_style));
            spans.push(Span::styled(
                if i + 1 < col_count { "┬" } else { "┐" }.to_string(),
                border_style,
            ));
        }
        out.push(Line::from(spans));
    }

    // │ Header │ Header │
    {
        let mut spans = vec![Span::styled("│".to_string(), border_style)];
        for (i, &w) in col_widths.iter().enumerate() {
            let cell = trunc(header.get(i).map(|s| s.as_str()).unwrap_or(""), w);
            let align = alignments.get(i).copied().unwrap_or(Alignment::None);
            let padded = pad_cell(&cell, w, align);
            spans.push(Span::raw(" "));
            spans.push(Span::styled(padded, header_style));
            spans.push(Span::raw(" "));
            spans.push(Span::styled("│".to_string(), border_style));
        }
        out.push(Line::from(spans));
    }

    // ├───┼───┤
    {
        let mut spans = vec![Span::styled("├".to_string(), border_style)];
        for (i, &w) in col_widths.iter().enumerate() {
            spans.push(Span::styled("─".repeat(w + 2), border_style));
            spans.push(Span::styled(
                if i + 1 < col_count { "┼" } else { "┤" }.to_string(),
                border_style,
            ));
        }
        out.push(Line::from(spans));
    }

    // │ cell │ cell │
    for row in rows {
        let mut spans = vec![Span::styled("│".to_string(), border_style)];
        for (i, &w) in col_widths.iter().enumerate() {
            let cell = trunc(row.get(i).map(|s| s.as_str()).unwrap_or(""), w);
            let align = alignments.get(i).copied().unwrap_or(Alignment::None);
            let padded = pad_cell(&cell, w, align);
            spans.push(Span::raw(" "));
            spans.push(Span::raw(padded));
            spans.push(Span::raw(" "));
            spans.push(Span::styled("│".to_string(), border_style));
        }
        out.push(Line::from(spans));
    }

    // └───┴───┘
    {
        let mut spans = vec![Span::styled("└".to_string(), border_style)];
        for (i, &w) in col_widths.iter().enumerate() {
            spans.push(Span::styled("─".repeat(w + 2), border_style));
            spans.push(Span::styled(
                if i + 1 < col_count { "┴" } else { "┘" }.to_string(),
                border_style,
            ));
        }
        out.push(Line::from(spans));
    }

    out
}

fn line_to_owned(line: Line<'_>) -> Line<'static> {
    let style = line.style;
    let alignment = line.alignment;
    let mut owned = Line::from(line.spans.into_iter().map(span_to_owned).collect::<Vec<_>>());
    owned.style = style;
    owned.alignment = alignment;
    owned
}

fn span_to_owned(span: Span<'_>) -> Span<'static> {
    Span::styled(span.content.into_owned(), span.style)
}

fn line_char_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
        .sum()
}

/// Word-wrap a styled `Line` to fit within `max_w` characters, preserving span and line styles.
fn wrap_styled_line(line: Line<'static>, max_w: usize) -> Vec<Line<'static>> {
    if max_w == 0 {
        return vec![line];
    }

    let line_style = line.style;
    let mut result: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut current_len: usize = 0;

    for span in line.spans {
        let style = span.style;
        let text: String = span.content.into_owned();

        let mut remaining = text.as_str();
        while !remaining.is_empty() {
            let space_left = max_w.saturating_sub(current_len);
            if space_left == 0 {
                let mut l = Line::from(std::mem::take(&mut current_spans));
                l.style = line_style;
                result.push(l);
                current_len = 0;
                continue;
            }

            let chunk_end = split_at_width(remaining, space_left);

            let chunk = &remaining[..chunk_end];
            current_spans.push(Span::styled(chunk.to_string(), style));
            current_len += UnicodeWidthStr::width(chunk);
            remaining = &remaining[chunk_end..];

            if !remaining.is_empty() {
                let mut l = Line::from(std::mem::take(&mut current_spans));
                l.style = line_style;
                result.push(l);
                current_len = 0;
            }
        }
    }

    if !current_spans.is_empty() {
        let mut l = Line::from(current_spans);
        l.style = line_style;
        result.push(l);
    }

    if result.is_empty() {
        let mut l = Line::from("");
        l.style = line_style;
        result.push(l);
    }

    result
}

/// Byte index at which `s` should be split so the prefix fits in `max_cols` terminal
/// columns. Wide characters (emoji, CJK, …) count as 2 columns; combining/control
/// chars count as 0 or 1.  Always advances by at least one character so callers
/// cannot loop infinitely when a single character exceeds the budget.
fn split_at_width(s: &str, max_cols: usize) -> usize {
    use unicode_width::UnicodeWidthChar;
    let mut cols = 0usize;
    for (byte_idx, ch) in s.char_indices() {
        let w = ch.width().unwrap_or(1);
        if cols + w > max_cols {
            // Force advance if nothing has fit yet to prevent infinite loops.
            return if cols == 0 { byte_idx + ch.len_utf8() } else { byte_idx };
        }
        cols += w;
    }
    s.len()
}

fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut remaining = text;
    loop {
        let byte_idx = split_at_width(remaining, max_width);
        if byte_idx >= remaining.len() {
            break;
        }
        lines.push(remaining[..byte_idx].to_string());
        remaining = &remaining[byte_idx..];
    }
    lines.push(remaining.to_string());
    lines
}

// ---------------------------------------------------------------------------
// Components panel
// ---------------------------------------------------------------------------

fn render_components_panel(frame: &mut Frame, model: &mut Model, area: Rect) {
    let style = panel_style(&model.theme, model.active, ActivePanel::Components);

    let outer_block = Block::default()
        .title(" Components [◀-▶ switch tab] ")
        .title_bottom(" [i import] [d del] [Ctrl+R refresh] ")
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
        Line::from("◀ Tools"),
        Line::from("Skills"),
        Line::from("Plugins ▶"),
    ])
    .select(match model.components_tab {
        ComponentsTab::Tools => 0,
        ComponentsTab::Skills => 1,
        ComponentsTab::Plugins => 2,
    })
    .highlight_style(
        Style::default()
            .fg(model.theme.text_primary)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(tabs, tabs_area);

    let t = model.theme.clone();
    let (loading, error, label, items): (bool, Option<&str>, &str, Vec<(String, String, bool)>) =
        match model.components_tab {
            ComponentsTab::Plugins => (
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
            ),
            ComponentsTab::Skills => (
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
            ),
            ComponentsTab::Tools => (
                model.components_tools_loading,
                model.components_tools_error.as_deref(),
                "tools",
                model
                    .components_tools
                    .iter()
                    .map(|t| (t.id.clone(), t.description.clone(), t.readonly))
                    .collect(),
            ),
        };

    render_components_list(
        frame,
        content_area,
        loading,
        error,
        label,
        items,
        &mut model.components_list_state,
        model.components_selected_index,
        &t,
    );
}

fn render_components_list(
    frame: &mut Frame,
    area: Rect,
    loading: bool,
    error: Option<&str>,
    label: &str,
    items: Vec<(String, String, bool)>,
    list_state: &mut ListState,
    selected_index: usize,
    theme: &Theme,
) {
    if loading {
        let msg = Paragraph::new(format!("Loading {label}..."))
            .style(Style::default().fg(theme.text_subtle));
        frame.render_widget(msg, area);
        return;
    }
    if let Some(err) = error {
        let msg = Paragraph::new(format!("Error: {err}"))
            .style(Style::default().fg(theme.text_error))
            .wrap(Wrap { trim: false });
        frame.render_widget(msg, area);
        return;
    }
    if items.is_empty() {
        let msg = Paragraph::new(format!("No {label} available."))
            .style(Style::default().fg(theme.text_subtle));
        frame.render_widget(msg, area);
        return;
    }

    let list_items: Vec<ListItem> = items
        .iter()
        .map(|(name, desc, readonly)| {
            let tag = if *readonly { " (built-in)" } else { "" };
            let line = Line::from(vec![
                Span::styled("• ", Style::default().fg(theme.text_primary)),
                Span::raw(format!("{name} ")),
                Span::styled(
                    format!("{desc}{tag}"),
                    Style::default().fg(theme.text_subtle),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let count = list_items.len();
    let list = List::new(list_items)
        .highlight_style(
            Style::default()
                .bg(theme.highlight_bg)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let title_line = Line::from(Span::styled(
        format!(" {count} {label} "),
        Style::default().fg(theme.text_subtle),
    ));
    let header = Paragraph::new(title_line);

    if area.height < 2 {
        frame.render_stateful_widget(list, area, list_state);
        return;
    }

    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    frame.render_widget(header, split[0]);

    let list_area = split[1];
    let content_area = Rect {
        width: list_area.width.saturating_sub(1),
        ..list_area
    };
    frame.render_stateful_widget(list, content_area, list_state);

    let total = count;
    let viewport = list_area.height as usize;
    if total > viewport {
        let scrollbar_area = Rect {
            x: list_area.x + list_area.width.saturating_sub(1),
            width: 1,
            ..list_area
        };
        let mut scrollbar_state = ScrollbarState::default()
            .content_length(total)
            .viewport_content_length(viewport)
            .position(selected_index);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            scrollbar_area,
            &mut scrollbar_state,
        );
    }
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

fn render_modal(frame: &mut Frame, modal: &mut Modal, theme: &Theme) {
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
                        .border_style(Style::default().fg(theme.text_warning)),
                )
                .wrap(Wrap { trim: false });
            frame.render_widget(widget, rect);
        }

        Modal::Info { title, message } => {
            render_simple_modal(frame, title, message, theme.text_primary);
        }

        Modal::Error { title, message } => {
            render_simple_modal(frame, title, message, theme.text_error);
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
                        .border_style(Style::default().fg(theme.text_error)),
                )
                .wrap(Wrap { trim: false });
            frame.render_widget(widget, rect);
        }

        Modal::ConfirmDeleteConversation(state) => {
            render_confirm_delete_conversation_modal(frame, state, theme);
        }

        Modal::CreateAgent(state) => {
            render_create_agent_modal(frame, state, theme);
        }

        Modal::ImportChooser(state) => {
            render_import_chooser_modal(frame, state, theme);
        }

        Modal::Import(state) => {
            render_import_modal(frame, state, theme);
        }

        Modal::InstallPlugin(state) => {
            render_install_plugin_modal(frame, state, theme);
        }

        Modal::GitHubImport(state) => {
            render_github_import_modal(frame, state, theme);
        }

        Modal::ConfirmDeleteComponent(state) => {
            render_confirm_delete_component_modal(frame, state, theme);
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

fn render_import_chooser_modal(frame: &mut Frame, state: &ImportChooserModal, theme: &Theme) {
    let rect = centered_rect(40, 12, frame.area());
    frame.render_widget(Clear, rect);

    let title = match state.target {
        ImportTarget::Agent => " Import Agent ",
        ImportTarget::Component(ComponentsTab::Tools) => " Import Tool ",
        ImportTarget::Component(ComponentsTab::Skills) => " Import Skill ",
        ImportTarget::Component(ComponentsTab::Plugins) => " Install Plugin ",
    };

    let outer_block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.text_primary));

    let inner = outer_block.inner(rect);
    frame.render_widget(outer_block, rect);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    let is_plugin = state.target == ImportTarget::Component(ComponentsTab::Plugins);

    let mut options: Vec<&str> = Vec::new();
    if !is_plugin {
        options.push("From Disk");
    }
    options.push("From GitHub URL");

    let items: Vec<ListItem> = options
        .iter()
        .map(|label| {
            ListItem::new(Line::from(format!("  {label}")))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(theme.highlight_bg)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected));
    frame.render_stateful_widget(list, layout[0], &mut list_state);

    let help = Paragraph::new("[Enter] Select  [Esc] Cancel")
        .style(Style::default().fg(theme.text_subtle));
    frame.render_widget(help, layout[1]);
}

fn render_confirm_delete_conversation_modal(frame: &mut Frame, state: &ConfirmDeleteConversationModal, theme: &Theme) {
    let rect = centered_rect(50, 25, frame.area());
    frame.render_widget(Clear, rect);
    let msg = if state.deleting {
        format!("Deleting \"{}\"...", state.conversation_title)
    } else {
        format!(
            "Delete conversation \"{}\"?\n\nThis will permanently remove it from Kibana.\n\n[y] Yes  [n/Esc] Cancel",
            state.conversation_title
        )
    };
    let widget = Paragraph::new(msg)
        .block(
            Block::default()
                .title(" Delete Conversation ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.text_error)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, rect);
}

fn render_confirm_delete_component_modal(frame: &mut Frame, state: &ConfirmDeleteComponentModal, theme: &Theme) {
    let rect = centered_rect(55, 30, frame.area());
    frame.render_widget(Clear, rect);

    let type_label = match state.component_tab {
        ComponentsTab::Tools => "tool",
        ComponentsTab::Skills => "skill",
        ComponentsTab::Plugins => "plugin",
    };

    let msg = if state.deleting {
        format!("Deleting \"{}\"...", state.component_name)
    } else if let Some(agents) = &state.in_use_by {
        let agent_list = agents.join(", ");
        format!(
            "This {type_label} is in use by: {agent_list}\n\nDelete anyway? This will remove it from those agents.\n\n[y] Yes  [n/Esc] Cancel"
        )
    } else {
        format!(
            "Delete {type_label} \"{}\"?\n\nNote: this {type_label} may be in use by one or more agents.\n\n[y] Yes  [n/Esc] Cancel",
            state.component_name
        )
    };

    let title = if state.in_use_by.is_some() {
        format!(" {type_label} In Use ")
    } else {
        format!(" Delete {} ", capitalize(type_label))
    };

    let widget = Paragraph::new(msg)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.text_error)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, rect);
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

fn render_import_modal(frame: &mut Frame, state: &ImportModal, theme: &Theme) {
    let rect = centered_rect(60, 70, frame.area());
    frame.render_widget(Clear, rect);

    let type_label = match state.target {
        ImportTarget::Agent => "Agent",
        ImportTarget::Component(ComponentsTab::Plugins) => "Plugin",
        ImportTarget::Component(ComponentsTab::Skills) => "Skill",
        ImportTarget::Component(ComponentsTab::Tools) => "Tool",
    };

    let outer_block = Block::default()
        .title(format!(" Import {type_label} from YAML "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.text_primary));

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
            Style::default().fg(theme.text_error),
        ));
    }
    footer_lines.push(Line::styled(
        "[Enter] Select  [Esc] Cancel  [↑↓] Navigate  [←/Backspace] Up  [→/Enter] Open dir",
        Style::default().fg(theme.text_subtle),
    ));

    let footer = Paragraph::new(footer_lines).wrap(Wrap { trim: false });
    frame.render_widget(footer, footer_area);
}

fn render_install_plugin_modal(frame: &mut Frame, state: &InstallPluginModal, theme: &Theme) {
    let rect = centered_rect(60, 30, frame.area());
    frame.render_widget(Clear, rect);

    let outer_block = Block::default()
        .title(" Install Plugin from URL ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.text_primary));

    let inner = outer_block.inner(rect);
    frame.render_widget(outer_block, rect);

    if inner.height < 4 || inner.width < 10 {
        return;
    }

    let has_error = state.error_message.is_some();
    let mut constraints = vec![
        Constraint::Length(1), // label
        Constraint::Length(1), // gap
        Constraint::Length(3), // input box
    ];
    if has_error {
        constraints.push(Constraint::Length(1)); // error line
    }
    if state.installing {
        constraints.push(Constraint::Length(1)); // status line
    }
    constraints.push(Constraint::Length(1)); // help line
    constraints.push(Constraint::Min(0)); // spacer

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let label = Paragraph::new("Enter a GitHub URL or direct ZIP URL:");
    frame.render_widget(label, layout[0]);

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_normal));
    let input_inner = input_block.inner(layout[2]);
    let input = Paragraph::new(state.url_buffer.as_str()).block(input_block);
    frame.render_widget(input, layout[2]);

    if !state.installing {
        let cursor_x = input_inner.x + state.url_buffer[..state.cursor].chars().count() as u16;
        frame.set_cursor_position(Position::new(cursor_x.min(input_inner.right().saturating_sub(1)), input_inner.y));
    }

    let mut row = 3;
    if has_error {
        let err = Paragraph::new(state.error_message.as_deref().unwrap_or(""))
            .style(Style::default().fg(theme.text_error));
        frame.render_widget(err, layout[row]);
        row += 1;
    }
    if state.installing {
        let status = Paragraph::new("Installing...")
            .style(Style::default().fg(theme.text_warning));
        frame.render_widget(status, layout[row]);
        row += 1;
    }
    let help = Paragraph::new("[Enter] Install  [Esc] Cancel")
        .style(Style::default().fg(theme.text_subtle));
    frame.render_widget(help, layout[row]);
}

fn render_github_import_modal(frame: &mut Frame, state: &GitHubImportModal, theme: &Theme) {
    let rect = centered_rect(65, 15, frame.area());
    frame.render_widget(Clear, rect);

    let type_label = match state.target {
        ImportTarget::Agent => "Agent",
        ImportTarget::Component(ComponentsTab::Plugins) => "Plugin",
        ImportTarget::Component(ComponentsTab::Skills) => "Skill",
        ImportTarget::Component(ComponentsTab::Tools) => "Tool",
    };

    let outer_block = Block::default()
        .title(format!(" Import {type_label} from GitHub "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.text_primary));

    let inner = outer_block.inner(rect);
    frame.render_widget(outer_block, rect);

    if inner.height < 5 || inner.width < 10 {
        return;
    }

    let has_error = state.error_message.is_some();
    let mut constraints = vec![
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(3),
    ];
    if has_error {
        constraints.push(Constraint::Length(1));
    }
    if state.importing {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(1));
    constraints.push(Constraint::Min(0));

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let label = Paragraph::new("Paste a GitHub file or folder URL:");
    frame.render_widget(label, layout[0]);

    let example_text = match state.target {
        ImportTarget::Agent => "e.g. https://github.com/org/repo/tree/main/agents/my-agent/my-agent.yaml",
        ImportTarget::Component(ComponentsTab::Tools) => "e.g. https://github.com/org/repo/blob/main/tools/my-tool/my-tool.yaml",
        ImportTarget::Component(ComponentsTab::Skills) => "e.g. https://github.com/org/repo/tree/main/skills/my-skill/my-skill.yaml",
        ImportTarget::Component(ComponentsTab::Plugins) => "e.g. https://github.com/org/repo/plugin.zip",
    };
    let example = Paragraph::new(example_text).style(Style::default().fg(theme.text_subtle));
    frame.render_widget(example, layout[1]);

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_normal));
    let input_inner = input_block.inner(layout[3]);
    let input = Paragraph::new(state.url_buffer.as_str()).block(input_block);
    frame.render_widget(input, layout[3]);

    if !state.importing {
        let cursor_x = input_inner.x + state.url_buffer[..state.cursor].chars().count() as u16;
        frame.set_cursor_position(Position::new(
            cursor_x.min(input_inner.right().saturating_sub(1)),
            input_inner.y,
        ));
    }

    let mut row = 4;
    if has_error {
        let err = Paragraph::new(state.error_message.as_deref().unwrap_or(""))
            .style(Style::default().fg(theme.text_error));
        frame.render_widget(err, layout[row]);
        row += 1;
    }
    if state.importing {
        let status =
            Paragraph::new("Fetching from GitHub...").style(Style::default().fg(theme.text_warning));
        frame.render_widget(status, layout[row]);
        row += 1;
    }
    let help =
        Paragraph::new("[Enter] Import  [Esc] Cancel").style(Style::default().fg(theme.text_subtle));
    frame.render_widget(help, layout[row]);
}

fn render_create_agent_modal(frame: &mut Frame, state: &mut CreateAgentModal, theme: &Theme) {
    let rect = centered_rect(70, 70, frame.area());
    frame.render_widget(Clear, rect);

    let title = match &state.mode {
        AgentEditorMode::Create => " Create Agent ",
        AgentEditorMode::Edit { .. } => " Edit Agent ",
    };

    let outer_block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.text_primary));

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
    .highlight_style(Style::default().fg(theme.text_primary).add_modifier(Modifier::BOLD));
    frame.render_widget(tabs, tabs_area);

    match state.tab {
        CreateAgentTab::Prompt => render_prompt_tab(frame, state, content_area, theme),
        CreateAgentTab::Tools => render_tools_tab(frame, state, content_area, theme),
        CreateAgentTab::Skills => render_skills_tab(frame, state, content_area, theme),
        CreateAgentTab::Plugins => render_plugins_tab(frame, state, content_area, theme),
    }

    let mut help_lines = vec![Line::from(
        "Tab: next field | ◀-▶: switch tab | Ctrl+S: save | Esc: cancel",
    )];
    if state.submitting {
        help_lines.push(Line::styled(
            "Saving...",
            Style::default().fg(theme.text_warning),
        ));
    }
    if let Some(err) = &state.error {
        help_lines.push(Line::styled(
            err.as_str(),
            Style::default().fg(theme.text_error),
        ));
    }
    let help = Paragraph::new(help_lines)
        .style(Style::default().fg(theme.text_subtle))
        .wrap(Wrap { trim: false });
    frame.render_widget(help, help_area);
}

fn render_prompt_tab(frame: &mut Frame, state: &CreateAgentModal, area: Rect, theme: &Theme) {
    let fields = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(area);

    let focus_style = Style::default().fg(theme.text_primary);
    let normal_style = Style::default().fg(theme.border_normal);

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

    let ec_style = if state.focus == CreateAgentField::ElasticCapabilities {
        focus_style
    } else {
        normal_style
    };
    let toggle = if state.enable_elastic_capabilities {
        "[x] Enabled"
    } else {
        "[ ] Disabled"
    };
    let ec_widget = Paragraph::new(format!("  {toggle}  (Space/Enter to toggle)")).block(
        Block::default()
            .title(" Elastic Capabilities ")
            .borders(Borders::ALL)
            .border_style(ec_style),
    );
    frame.render_widget(ec_widget, fields[3]);
}

fn render_tools_tab(frame: &mut Frame, state: &mut CreateAgentModal, area: Rect, theme: &Theme) {
    if state.tools_loading {
        let msg = Paragraph::new("Loading tools...").style(Style::default().fg(theme.text_subtle));
        frame.render_widget(msg, area);
        return;
    }
    if let Some(err) = &state.tools_error {
        let msg = Paragraph::new(format!("Error: {err}"))
            .style(Style::default().fg(theme.text_error))
            .wrap(Wrap { trim: false });
        frame.render_widget(msg, area);
        return;
    }
    if state.tools.is_empty() {
        let msg = Paragraph::new("No tools available.").style(Style::default().fg(theme.text_subtle));
        frame.render_widget(msg, area);
        return;
    }

    let items: Vec<ListItem> = state
        .tools
        .iter()
        .map(|t| {
            let checked = if state.selected_tool_ids.contains(&t.id) { "[x]" } else { "[ ]" };
            let line = Line::from(vec![
                Span::styled(format!("{checked} "), Style::default().fg(theme.text_primary)),
                Span::raw(&t.id),
                Span::styled(format!("  {}", t.description), Style::default().fg(theme.text_subtle)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .highlight_style(Style::default().bg(theme.highlight_bg).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, &mut state.tools_list_state);
}

fn render_skills_tab(frame: &mut Frame, state: &mut CreateAgentModal, area: Rect, theme: &Theme) {
    if state.skills_loading {
        let msg = Paragraph::new("Loading skills...").style(Style::default().fg(theme.text_subtle));
        frame.render_widget(msg, area);
        return;
    }
    if let Some(err) = &state.skills_error {
        let msg = Paragraph::new(format!("Error: {err}"))
            .style(Style::default().fg(theme.text_error))
            .wrap(Wrap { trim: false });
        frame.render_widget(msg, area);
        return;
    }
    if state.skills.is_empty() {
        let msg = Paragraph::new("No skills available.").style(Style::default().fg(theme.text_subtle));
        frame.render_widget(msg, area);
        return;
    }

    let items: Vec<ListItem> = state
        .skills
        .iter()
        .map(|s| {
            let checked = if state.selected_skill_ids.contains(&s.id) { "[x]" } else { "[ ]" };
            let plugin_tag = s.plugin_id.as_deref().map(|_| " (plugin)").unwrap_or("");
            let readonly_tag = if s.readonly { " (built-in)" } else { "" };
            let line = Line::from(vec![
                Span::styled(format!("{checked} "), Style::default().fg(theme.text_primary)),
                Span::raw(format!("{} ", s.name)),
                Span::styled(
                    format!("{}{readonly_tag}{plugin_tag}", s.description),
                    Style::default().fg(theme.text_subtle),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .highlight_style(Style::default().bg(theme.highlight_bg).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, &mut state.skills_list_state);
}

fn render_plugins_tab(frame: &mut Frame, state: &mut CreateAgentModal, area: Rect, theme: &Theme) {
    if state.plugins_loading {
        let msg = Paragraph::new("Loading plugins...").style(Style::default().fg(theme.text_subtle));
        frame.render_widget(msg, area);
        return;
    }
    if let Some(err) = &state.plugins_error {
        let msg = Paragraph::new(format!("Error: {err}"))
            .style(Style::default().fg(theme.text_error))
            .wrap(Wrap { trim: false });
        frame.render_widget(msg, area);
        return;
    }
    if state.plugins.is_empty() {
        let msg = Paragraph::new("No plugins available.").style(Style::default().fg(theme.text_subtle));
        frame.render_widget(msg, area);
        return;
    }

    let items: Vec<ListItem> = state
        .plugins
        .iter()
        .map(|p| {
            let checked = if state.selected_plugin_ids.contains(&p.id) { "[x]" } else { "[ ]" };
            let skills_info = if p.skill_ids.is_empty() { String::new() } else { format!(" ({} skills)", p.skill_ids.len()) };
            let version_tag = if p.version.is_empty() { String::new() } else { format!(" v{}", p.version) };
            let line = Line::from(vec![
                Span::styled(format!("{checked} "), Style::default().fg(theme.text_primary)),
                Span::raw(format!("{} ", p.name)),
                Span::styled(
                    format!("{}{version_tag}{skills_info}", p.description),
                    Style::default().fg(theme.text_subtle),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .highlight_style(Style::default().bg(theme.highlight_bg).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, &mut state.plugins_list_state);
}

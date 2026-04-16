use std::collections::HashSet;

use ratatui::crossterm::event::{KeyCode, KeyModifiers, MouseEventKind};
use ratatui::widgets::ListState;

use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders};
use ratatui_explorer::{FileExplorerBuilder, Theme as ExplorerTheme};

use super::cmd::Cmd;
use super::model::{
    ActivePanel, AgentEditorMode, ChatEntry, ChatRole, ChatSession, ComponentsTab,
    ConfirmDeleteAgentModal, ConfirmDeleteConversationModal, CreateAgentField, CreateAgentModal, CreateAgentTab,
    GitHubImportAgentModal, GitHubImportModal, ImportAgentModal, ImportModal, InstallPluginModal,
    Modal, Model,
};
use super::msg::Msg;
use super::view::filtered_session_indices;

pub fn update(model: &mut Model, msg: Msg) -> Vec<Cmd> {
    // When a modal is open, keyboard input goes to the modal first.
    if model.modal.is_some() {
        match msg {
            Msg::Quit => {
                model.should_quit = true;
                return vec![];
            }
            Msg::Key(key) => return update_modal_key(model, key),
            _ => {} // fall through to handle background messages
        }
    }

    match msg {
        Msg::Init => vec![Cmd::LoadEnv],

        Msg::Quit => {
            model.should_quit = true;
            vec![]
        }

        Msg::Key(key) => {
            // Panel-specific key handling.
            if model.active == ActivePanel::Chat
                && let Some(cmds) = handle_chat_input_key(model, key)
            {
                return cmds;
            }

            if model.active == ActivePanel::Agents
                && let Some(cmds) = handle_agents_panel_key(model, key)
            {
                return cmds;
            }

            if model.active == ActivePanel::Chats
                && let Some(cmds) = handle_chats_panel_key(model, key)
            {
                return cmds;
            }

            if model.active == ActivePanel::Components
                && let Some(cmds) = handle_components_panel_key(model, key)
            {
                return cmds;
            }

            match key.code {
                KeyCode::Char('q') if key.modifiers.is_empty() => {
                    model.should_quit = true;
                }
                KeyCode::Tab => {
                    model.active = match model.active {
                        ActivePanel::Agents => ActivePanel::Chats,
                        ActivePanel::Chats => ActivePanel::Chat,
                        ActivePanel::Chat => ActivePanel::Components,
                        ActivePanel::Components => ActivePanel::Agents,
                    };
                }
                KeyCode::BackTab => {
                    model.active = match model.active {
                        ActivePanel::Agents => ActivePanel::Components,
                        ActivePanel::Chats => ActivePanel::Agents,
                        ActivePanel::Chat => ActivePanel::Chats,
                        ActivePanel::Components => ActivePanel::Chat,
                    };
                }
                _ => {}
            }
            vec![]
        }

        // -- Config --
        Msg::EnvLoaded { config } => {
            let theme_path = config.theme_path.clone();
            model.config = std::sync::Arc::new(config);
            model.env_loaded = true;

            if let Some(path) = &theme_path {
                model.theme = crate::theme::Theme::load(path);
            } else if std::path::Path::new("themes/elastic-borealis.yaml").exists() {
                model.theme = crate::theme::Theme::load("themes/elastic-borealis.yaml");
            }
            if !model.config.is_ready() {
                model.modal = Some(Modal::MissingEnv {
                    missing: model.config.missing(),
                });
                return vec![];
            }
            model.agents_loading = true;
            model.agents_generation += 1;
            model.components_tools_loading = true;
            model.components_skills_loading = true;
            model.components_plugins_loading = true;
            model.components_generation += 1;
            vec![Cmd::LoadAgents, Cmd::LoadComponentsData]
        }

        // -- Agents --
        Msg::AgentsLoaded { agents, generation } => {
            if generation < model.agents_generation {
                return vec![];
            }
            model.agents_loading = false;
            model.agents_loaded = true;
            model.agents_error = None;
            model.agents = agents;

            if let Some(sel_id) = &model.selected_agent_id {
                if let Some(idx) = model.agents.iter().position(|a| &a.id == sel_id) {
                    model.agent_selected_index = idx;
                } else {
                    model.agent_selected_index = 0;
                }
            } else {
                model.agent_selected_index = 0;
            }
            sync_agents_list_state(model);

            if !model.conversations_loading && model.sessions.iter().all(|s| !s.from_server) {
                model.conversations_loading = true;
                return vec![Cmd::LoadConversations];
            }
            vec![]
        }

        Msg::AgentsLoadFailed { error, generation } => {
            if generation < model.agents_generation {
                return vec![];
            }
            model.agents_loading = false;
            model.agents_error = Some(error);
            vec![]
        }

        // -- Tools (for create/edit modal) --
        Msg::ToolsLoaded { tools } => {
            if let Some(Modal::CreateAgent(state)) = &mut model.modal {
                state.tools_loading = false;
                state.tools_error = None;
                let valid_ids: HashSet<&str> = tools.iter().map(|t| t.id.as_str()).collect();
                state.selected_tool_ids.retain(|id| valid_ids.contains(id.as_str()));
                state.tools = tools;
            }
            vec![]
        }

        Msg::ToolsLoadFailed { error } => {
            if let Some(Modal::CreateAgent(state)) = &mut model.modal {
                state.tools_loading = false;
                state.tools_error = Some(error);
            }
            vec![]
        }

        Msg::SkillsLoaded { skills } => {
            if let Some(Modal::CreateAgent(state)) = &mut model.modal {
                state.skills_loading = false;
                state.skills_error = None;
                let valid_ids: HashSet<&str> = skills.iter().map(|s| s.id.as_str()).collect();
                state.selected_skill_ids.retain(|id| valid_ids.contains(id.as_str()));
                state.skills = skills;
            }
            vec![]
        }

        Msg::SkillsLoadFailed { error } => {
            if let Some(Modal::CreateAgent(state)) = &mut model.modal {
                state.skills_loading = false;
                state.skills_error = Some(error);
            }
            vec![]
        }

        Msg::PluginsLoaded { plugins } => {
            if let Some(Modal::CreateAgent(state)) = &mut model.modal {
                state.plugins_loading = false;
                state.plugins_error = None;
                let valid_ids: HashSet<&str> = plugins.iter().map(|p| p.id.as_str()).collect();
                state.selected_plugin_ids.retain(|id| valid_ids.contains(id.as_str()));
                state.plugins = plugins;
            }
            vec![]
        }

        Msg::PluginsLoadFailed { error } => {
            if let Some(Modal::CreateAgent(state)) = &mut model.modal {
                state.plugins_loading = false;
                state.plugins_error = Some(error);
            }
            vec![]
        }

        // -- Components panel data --
        Msg::ComponentsDataLoaded {
            tools,
            skills,
            plugins,
            generation,
        } => {
            if generation < model.components_generation {
                return vec![];
            }
            model.components_tools_loading = false;
            model.components_skills_loading = false;
            model.components_plugins_loading = false;
            model.components_tools_error = None;
            model.components_skills_error = None;
            model.components_plugins_error = None;
            model.components_tools = tools;
            model.components_skills = skills;
            model.components_plugins = plugins;
            model.components_selected_index = 0;
            sync_components_list_state(model);
            vec![]
        }

        Msg::ComponentsDataFailed { error, generation } => {
            if generation < model.components_generation {
                return vec![];
            }
            model.components_tools_loading = false;
            model.components_skills_loading = false;
            model.components_plugins_loading = false;
            model.components_tools_error = Some(error.clone());
            model.components_skills_error = Some(error.clone());
            model.components_plugins_error = Some(error);
            vec![]
        }

        // -- Agent CRUD results --
        Msg::AgentUpserted { agent, is_edit } => {
            if let Some(session) = model.active_session_mut() {
                session.push_chat(ChatEntry {
                    role: ChatRole::System,
                    content: format!(
                        "{} agent: {} ({})",
                        if is_edit { "Updated" } else { "Created" },
                        agent.name,
                        agent.id
                    ),
                    steps: Vec::new(),
                });
            }
            if matches!(
                model.modal,
                Some(Modal::CreateAgent(_) | Modal::ImportAgent(_) | Modal::GitHubImportAgent(_))
            ) {
                model.modal = None;
            }
            model.selected_agent_id = Some(agent.id.clone());
            std::sync::Arc::make_mut(&mut model.config).agent_id = agent.id;

            model.agents_loaded = false;
            model.agents_loading = false;
            model.agents_error = None;
            model.agents.clear();
            model.agent_selected_index = 0;
            model.agents_list_state = ListState::default();
            model.agents_generation += 1;
            model.active = ActivePanel::Agents;
            vec![Cmd::LoadAgents]
        }

        Msg::AgentUpsertFailed { error } => {
            match &mut model.modal {
                Some(Modal::CreateAgent(state)) => {
                    state.submitting = false;
                    state.error = Some(error);
                }
                Some(Modal::GitHubImportAgent(_)) | Some(Modal::ImportAgent(_)) => {
                    model.modal = Some(Modal::Error {
                        title: "Agent Import Failed".to_string(),
                        message: error,
                    });
                }
                _ => {
                    model.modal = Some(Modal::Error {
                        title: "Agent Import Failed".to_string(),
                        message: error,
                    });
                }
            }
            vec![]
        }

        Msg::AgentDeleted { name } => {
            model.modal = None;
            if let Some(session) = model.active_session_mut() {
                session.push_chat(ChatEntry {
                    role: ChatRole::System,
                    content: format!("Deleted agent: {name}"),
                    steps: Vec::new(),
                });
            }
            model.agents_loaded = false;
            model.agents_loading = false;
            model.agents.clear();
            model.agent_selected_index = 0;
            model.agents_list_state = ListState::default();
            model.agents_generation += 1;
            vec![Cmd::LoadAgents]
        }

        Msg::AgentDeleteFailed { error } => {
            model.modal = Some(Modal::Error {
                title: "Delete failed".to_string(),
                message: error,
            });
            vec![]
        }

        Msg::ConversationDeleted { conversation_id } => {
            model.modal = None;
            // Remove the session that matches the deleted conversation.
            if let Some(idx) = model
                .sessions
                .iter()
                .position(|s| s.conversation_id.as_deref() == Some(&conversation_id))
            {
                model.sessions.remove(idx);
                match model.active_session_index {
                    Some(active) if active == idx => {
                        let filtered = filtered_session_indices(model);
                        model.active_session_index = filtered.first().copied();
                    }
                    Some(active) if active > idx => {
                        model.active_session_index = Some(active - 1);
                    }
                    _ => {}
                }
                sync_sessions_list_state(model);
            }
            vec![]
        }

        Msg::ConversationDeleteFailed { error } => {
            model.modal = Some(Modal::Error {
                title: "Delete failed".to_string(),
                message: error,
            });
            vec![]
        }

        // -- Import from file --
        Msg::ToolCreatedFromFile { tool } => {
            model.modal = Some(Modal::Info {
                title: "Tool Imported".to_string(),
                message: format!("Successfully created tool: {} ({})", tool.id, tool.tool_type),
            });
            model.components_tools_loading = true;
            model.components_skills_loading = true;
            model.components_plugins_loading = true;
            model.components_generation += 1;
            vec![Cmd::LoadComponentsData]
        }

        Msg::ToolCreateFromFileFailed { error } => {
            model.modal = Some(Modal::Error {
                title: "Import Failed".to_string(),
                message: error,
            });
            vec![]
        }

        Msg::SkillCreatedFromFile { skill } => {
            model.modal = Some(Modal::Info {
                title: "Skill Imported".to_string(),
                message: format!("Successfully created skill: {} ({})", skill.id, skill.name),
            });
            model.components_tools_loading = true;
            model.components_skills_loading = true;
            model.components_plugins_loading = true;
            model.components_generation += 1;
            vec![Cmd::LoadComponentsData]
        }

        Msg::SkillCreateFromFileFailed { error } => {
            model.modal = Some(Modal::Error {
                title: "Skill Import Failed".to_string(),
                message: error,
            });
            vec![]
        }

        Msg::PluginInstalledFromFile { plugin } => {
            model.modal = Some(Modal::Info {
                title: "Plugin Installed".to_string(),
                message: format!("Successfully installed plugin: {}", plugin.name),
            });
            model.components_tools_loading = true;
            model.components_skills_loading = true;
            model.components_plugins_loading = true;
            model.components_generation += 1;
            vec![Cmd::LoadComponentsData]
        }

        Msg::PluginInstallFromFileFailed { error } => {
            model.modal = Some(Modal::Error {
                title: "Plugin Import Failed".to_string(),
                message: error,
            });
            vec![]
        }

        // -- Conversations --
        Msg::ConversationsLoaded { conversations } => {
            model.conversations_loading = false;

            let server_ids: std::collections::HashSet<&str> =
                conversations.iter().map(|c| c.id.as_str()).collect();

            // Remove server-sourced sessions that no longer exist on the server.
            let mut i = 0;
            while i < model.sessions.len() {
                let sess = &model.sessions[i];
                let stale = sess.from_server
                    && sess
                        .conversation_id
                        .as_deref()
                        .is_some_and(|cid| !server_ids.contains(cid));
                if stale {
                    model.sessions.remove(i);
                    match model.active_session_index {
                        Some(active) if active == i => {
                            model.active_session_index = None;
                        }
                        Some(active) if active > i => {
                            model.active_session_index = Some(active - 1);
                        }
                        _ => {}
                    }
                } else {
                    i += 1;
                }
            }

            for conv in conversations {
                let already_exists = model
                    .sessions
                    .iter()
                    .any(|s| s.conversation_id.as_deref() == Some(&conv.id));
                if already_exists {
                    continue;
                }

                let agent_name = conv
                    .agent_id
                    .as_ref()
                    .and_then(|aid| model.agents.iter().find(|a| &a.id == aid))
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| "Unknown Agent".to_string());

                let title = conv
                    .title
                    .unwrap_or_else(|| "Untitled chat".to_string());

                model.sessions.push(ChatSession {
                    agent_id: conv.agent_id.unwrap_or_default(),
                    agent_name,
                    title,
                    conversation_id: Some(conv.id),
                    chat: Vec::new(),
                    input_buffer: String::new(),
                    input_cursor: 0,
                    waiting_for_response: false,
                    chat_scroll_from_bottom: 0,
                    from_server: true,
                    history_loaded: false,
                    history_loading: false,
                    model_name: None,
                });
            }

            model.enforce_session_cap();
            if model.active_session_index.is_none() && !model.sessions.is_empty() {
                model.active_session_index = Some(0);
            }
            sync_sessions_list_state(model);
            vec![]
        }

        Msg::ConversationsLoadFailed { error } => {
            model.conversations_loading = false;
            if let Some(session) = model.active_session_mut() {
                session.push_chat(ChatEntry {
                    role: ChatRole::System,
                    content: format!("Failed to load conversations: {error}"),
                    steps: Vec::new(),
                });
            }
            vec![]
        }

        Msg::ConversationHistoryLoaded {
            conversation_id,
            messages,
            model_name,
        } => {
            if let Some(session) = model
                .sessions
                .iter_mut()
                .find(|s| s.conversation_id.as_deref() == Some(&conversation_id))
            {
                session.history_loading = false;
                session.history_loaded = true;
                session.chat.clear();
                if model_name.is_some() {
                    session.model_name = model_name;
                }
                for (role, content, steps) in messages {
                    let chat_role = match role.as_str() {
                        "user" | "human" => ChatRole::User,
                        "assistant" | "ai" | "bot" => ChatRole::Assistant,
                        _ => ChatRole::System,
                    };
                    session.push_chat(ChatEntry {
                        role: chat_role,
                        content,
                        steps,
                    });
                }
            }
            vec![]
        }

        Msg::ConversationHistoryFailed {
            conversation_id,
            error,
        } => {
            if let Some(session) = model
                .sessions
                .iter_mut()
                .find(|s| s.conversation_id.as_deref() == Some(&conversation_id))
            {
                session.history_loading = false;
                session.push_chat(ChatEntry {
                    role: ChatRole::System,
                    content: format!("Failed to load history: {error}"),
                    steps: Vec::new(),
                });
            }
            vec![]
        }

        // -- Chat --
        Msg::PromptResponseReceived {
            content,
            conversation_id,
            model_name,
            steps,
        } => {
            if let Some(session) = model.active_session_mut() {
                session.waiting_for_response = false;
                session.conversation_id = conversation_id.or(session.conversation_id.take());
                if model_name.is_some() {
                    session.model_name = model_name;
                }
                session.push_chat(ChatEntry {
                    role: ChatRole::Assistant,
                    content,
                    steps,
                });
                session.chat_scroll_from_bottom = 0;
            }
            vec![]
        }

        Msg::PromptResponseFailed { error } => {
            if let Some(session) = model.active_session_mut() {
                session.waiting_for_response = false;
                session.push_chat(ChatEntry {
                    role: ChatRole::Assistant,
                    content: format!("Error: {error}"),
                    steps: Vec::new(),
                });
                session.chat_scroll_from_bottom = 0;
            }
            vec![]
        }

        Msg::Mouse(mouse) => {
            match mouse.kind {
                MouseEventKind::ScrollUp => scroll_chat(model, ScrollDir::Up, 3),
                MouseEventKind::ScrollDown => scroll_chat(model, ScrollDir::Down, 3),
                _ => {}
            }
            vec![]
        }

        Msg::Tick | Msg::Resize => vec![],
    }
}

// ---------------------------------------------------------------------------
// Agents panel key handling
// ---------------------------------------------------------------------------

fn handle_agents_panel_key(
    model: &mut Model,
    key: ratatui::crossterm::event::KeyEvent,
) -> Option<Vec<Cmd>> {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            if !model.agents.is_empty() {
                model.agent_selected_index = model.agent_selected_index.saturating_sub(1);
                sync_agents_list_state(model);
                snap_session_to_filtered(model);
            }
            Some(vec![])
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if !model.agents.is_empty() {
                model.agent_selected_index =
                    (model.agent_selected_index + 1).min(model.agents.len() - 1);
                sync_agents_list_state(model);
                snap_session_to_filtered(model);
            }
            Some(vec![])
        }
        KeyCode::Enter => {
            if !model.agents.is_empty() {
                let idx = model.agent_selected_index.min(model.agents.len() - 1);
                let agent_id = model.agents[idx].id.clone();
                let agent_name = model.agents[idx].name.clone();

                let session = ChatSession {
                    agent_id: agent_id.clone(),
                    agent_name: agent_name.clone(),
                    title: "New chat".to_string(),
                    conversation_id: None,
                    chat: vec![ChatEntry {
                        role: ChatRole::System,
                        content: format!("New chat with {agent_name} ({agent_id})"),
                        steps: Vec::new(),
                    }],
                    input_buffer: String::new(),
                    input_cursor: 0,
                    waiting_for_response: false,
                    chat_scroll_from_bottom: 0,
                    from_server: false,
                    history_loaded: true,
                    history_loading: false,
                    model_name: None,
                };
                model.sessions.push(session);
                let new_idx = model.sessions.len() - 1;
                model.active_session_index = Some(new_idx);
                model.enforce_session_cap();
                sync_sessions_list_state(model);

                model.selected_agent_id = Some(agent_id.clone());
                std::sync::Arc::make_mut(&mut model.config).agent_id = agent_id;
                model.active = ActivePanel::Chat;
            }
            Some(vec![])
        }
        KeyCode::Char('n') => {
            if !model.config.is_ready() {
                model.modal = Some(Modal::Info {
                    title: "Missing env".to_string(),
                    message: "Set KIBANA_URL and API_KEY before creating an agent.".to_string(),
                });
                return Some(vec![]);
            }
            let state = CreateAgentModal {
                tools_loading: true,
                skills_loading: true,
                plugins_loading: true,
                ..CreateAgentModal::default()
            };
            model.modal = Some(Modal::CreateAgent(Box::new(state)));
            Some(vec![Cmd::LoadTools, Cmd::LoadSkills, Cmd::LoadPlugins])
        }
        KeyCode::Char('e') => {
            if model.agents.is_empty() {
                return Some(vec![]);
            }
            if !model.config.is_ready() {
                model.modal = Some(Modal::Info {
                    title: "Missing env".to_string(),
                    message: "Set KIBANA_URL and API_KEY before editing an agent.".to_string(),
                });
                return Some(vec![]);
            }
            let idx = model.agent_selected_index.min(model.agents.len() - 1);
            let agent = model.agents[idx].clone();

            let mut selected_tool_ids = agent.tool_ids;
            if selected_tool_ids.is_empty() {
                selected_tool_ids = CreateAgentModal::default().selected_tool_ids;
            }
            let state = CreateAgentModal {
                mode: AgentEditorMode::Edit {
                    agent_id: agent.id.clone(),
                },
                name: agent.name,
                description: agent.description.unwrap_or_default(),
                instructions: agent.instructions.unwrap_or_default(),
                selected_tool_ids,
                selected_skill_ids: agent.skill_ids,
                selected_plugin_ids: agent.plugin_ids,
                enable_elastic_capabilities: agent.enable_elastic_capabilities,
                tools_loading: true,
                skills_loading: true,
                plugins_loading: true,
                tab: CreateAgentTab::Prompt,
                focus: CreateAgentField::Name,
                ..CreateAgentModal::default()
            };
            model.modal = Some(Modal::CreateAgent(Box::new(state)));
            Some(vec![Cmd::LoadTools, Cmd::LoadSkills, Cmd::LoadPlugins])
        }
        KeyCode::Char('d') => {
            if model.agents.is_empty() {
                return Some(vec![]);
            }
            let idx = model.agent_selected_index.min(model.agents.len() - 1);
            let agent = &model.agents[idx];
            model.modal = Some(Modal::ConfirmDeleteAgent(ConfirmDeleteAgentModal {
                agent_id: agent.id.clone(),
                agent_name: agent.name.clone(),
                deleting: false,
            }));
            Some(vec![])
        }
        KeyCode::Char('i') => {
            let fe_theme = ExplorerTheme::default()
                .add_default_title()
                .with_block(Block::default().borders(Borders::NONE))
                .with_item_style(Style::default().fg(model.theme.file_text))
                .with_dir_style(Style::default().fg(model.theme.file_dir))
                .with_highlight_item_style(
                    Style::default()
                        .fg(model.theme.file_text)
                        .bg(model.theme.file_highlight_bg)
                        .add_modifier(Modifier::BOLD),
                )
                .with_highlight_dir_style(
                    Style::default()
                        .fg(model.theme.file_dir)
                        .bg(model.theme.file_highlight_bg)
                        .add_modifier(Modifier::BOLD),
                )
                .with_highlight_symbol("▶ ");

            match FileExplorerBuilder::build_with_theme(fe_theme) {
                Ok(fe) => {
                    model.modal = Some(Modal::ImportAgent(Box::new(ImportAgentModal {
                        file_explorer: fe,
                        error_message: None,
                    })));
                }
                Err(e) => {
                    model.modal = Some(Modal::Error {
                        title: "File Explorer Error".to_string(),
                        message: format!("{e}"),
                    });
                }
            }
            Some(vec![])
        }
        KeyCode::Char('g') => {
            model.modal = Some(Modal::GitHubImportAgent(GitHubImportAgentModal {
                url_buffer: String::new(),
                cursor: 0,
                error_message: None,
                importing: false,
            }));
            Some(vec![])
        }
        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            model.agents_loading = true;
            model.agents_loaded = false;
            model.agents_error = None;
            model.agents_generation += 1;
            Some(vec![Cmd::LoadAgents])
        }
        _ => None,
    }
}

fn sync_agents_list_state(model: &mut Model) {
    if model.agents.is_empty() {
        model.agents_list_state.select(None);
    } else {
        model
            .agents_list_state
            .select(Some(model.agent_selected_index));
    }
}

// ---------------------------------------------------------------------------
// Chats panel key handling
// ---------------------------------------------------------------------------

fn handle_chats_panel_key(
    model: &mut Model,
    key: ratatui::crossterm::event::KeyEvent,
) -> Option<Vec<Cmd>> {
    let filtered = filtered_session_indices(model);

    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            if !filtered.is_empty() {
                let current_pos = model
                    .active_session_index
                    .and_then(|idx| filtered.iter().position(|&fi| fi == idx));
                match current_pos {
                    Some(pos) if pos > 0 => {
                        model.active_session_index = Some(filtered[pos - 1]);
                    }
                    None => {
                        model.active_session_index = Some(filtered[0]);
                    }
                    _ => {}
                }
                sync_sessions_list_state(model);
                activate_session_agent(model);
            }
            Some(vec![])
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if !filtered.is_empty() {
                let current_pos = model
                    .active_session_index
                    .and_then(|idx| filtered.iter().position(|&fi| fi == idx));
                match current_pos {
                    Some(pos) if pos + 1 < filtered.len() => {
                        model.active_session_index = Some(filtered[pos + 1]);
                    }
                    None => {
                        model.active_session_index = Some(filtered[0]);
                    }
                    _ => {}
                }
                sync_sessions_list_state(model);
                activate_session_agent(model);
            }
            Some(vec![])
        }
        KeyCode::Enter => {
            if model.active_session_index.is_some() {
                activate_session_agent(model);
                model.active = ActivePanel::Chat;

                if let Some(session) = model.active_session_mut()
                    && session.from_server
                    && !session.history_loaded
                    && !session.history_loading
                    && let Some(conv_id) = session.conversation_id.clone()
                {
                    session.history_loading = true;
                    return Some(vec![Cmd::LoadConversationHistory {
                        conversation_id: conv_id,
                    }]);
                }
            }
            Some(vec![])
        }
        KeyCode::Char('d') => {
            if let Some(active_idx) = model.active_session_index
                && active_idx < model.sessions.len()
            {
                let session = &model.sessions[active_idx];
                let conv_id = session.conversation_id.clone().unwrap_or_default();
                let title = session.title.clone();

                if conv_id.is_empty() {
                    // Local-only session (never saved to server) — just remove it.
                    model.sessions.remove(active_idx);
                    let new_filtered = filtered_session_indices(model);
                    if new_filtered.is_empty() {
                        model.active_session_index = None;
                    } else {
                        let old_pos = filtered
                            .iter()
                            .position(|&fi| fi == active_idx)
                            .unwrap_or(0);
                        let new_pos = old_pos.min(new_filtered.len() - 1);
                        model.active_session_index = Some(new_filtered[new_pos]);
                        activate_session_agent(model);
                    }
                    sync_sessions_list_state(model);
                } else {
                    model.modal = Some(Modal::ConfirmDeleteConversation(
                        ConfirmDeleteConversationModal {
                            conversation_id: conv_id,
                            conversation_title: title,
                            deleting: false,
                        },
                    ));
                }
            }
            Some(vec![])
        }
        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            model.conversations_loading = true;
            Some(vec![Cmd::LoadConversations])
        }
        _ => None,
    }
}

fn sync_sessions_list_state(model: &mut Model) {
    model.sessions_list_state.select(model.active_session_index);
}

/// When the agent selection changes, snap the active session to the first
/// chat in the new filtered view so the cursor stays visible.
fn snap_session_to_filtered(model: &mut Model) {
    let filtered = filtered_session_indices(model);
    let already_visible = model
        .active_session_index
        .is_some_and(|idx| filtered.contains(&idx));
    if !already_visible {
        model.active_session_index = filtered.first().copied();
        sync_sessions_list_state(model);
    }
}

/// When switching sessions, update the selected agent to match the session's agent.
fn activate_session_agent(model: &mut Model) {
    let agent_id = model
        .active_session()
        .map(|s| s.agent_id.clone());
    if let Some(agent_id) = agent_id {
        model.selected_agent_id = Some(agent_id.clone());
        std::sync::Arc::make_mut(&mut model.config).agent_id = agent_id;
    }
}

// ---------------------------------------------------------------------------
// Components panel helpers
// ---------------------------------------------------------------------------

fn components_list_len(model: &Model) -> usize {
    match model.components_tab {
        ComponentsTab::Plugins => model.components_plugins.len(),
        ComponentsTab::Skills => model.components_skills.len(),
        ComponentsTab::Tools => model.components_tools.len(),
    }
}

fn sync_components_list_state(model: &mut Model) {
    let len = components_list_len(model);
    if len == 0 {
        model.components_list_state.select(None);
    } else {
        model.components_selected_index = model.components_selected_index.min(len - 1);
        model
            .components_list_state
            .select(Some(model.components_selected_index));
    }
}

// ---------------------------------------------------------------------------
// Components panel key handling
// ---------------------------------------------------------------------------

fn handle_components_panel_key(
    model: &mut Model,
    key: ratatui::crossterm::event::KeyEvent,
) -> Option<Vec<Cmd>> {
    const TAB_ORDER: [ComponentsTab; 3] = [
        ComponentsTab::Tools,
        ComponentsTab::Skills,
        ComponentsTab::Plugins,
    ];

    match key.code {
        KeyCode::Left | KeyCode::Char('h') => {
            if let Some(pos) = TAB_ORDER.iter().position(|t| *t == model.components_tab)
                && pos > 0
            {
                model.components_tab = TAB_ORDER[pos - 1];
                model.components_selected_index = 0;
                sync_components_list_state(model);
            }
            Some(vec![])
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if let Some(pos) = TAB_ORDER.iter().position(|t| *t == model.components_tab)
                && pos + 1 < TAB_ORDER.len()
            {
                model.components_tab = TAB_ORDER[pos + 1];
                model.components_selected_index = 0;
                sync_components_list_state(model);
            }
            Some(vec![])
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let len = components_list_len(model);
            if len > 0 {
                model.components_selected_index =
                    model.components_selected_index.saturating_sub(1);
                sync_components_list_state(model);
            }
            Some(vec![])
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let len = components_list_len(model);
            if len > 0 {
                model.components_selected_index =
                    (model.components_selected_index + 1).min(len - 1);
                sync_components_list_state(model);
            }
            Some(vec![])
        }
        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            model.components_tools_loading = true;
            model.components_skills_loading = true;
            model.components_plugins_loading = true;
            model.components_generation += 1;
            Some(vec![Cmd::LoadComponentsData])
        }
        KeyCode::Char('i') => {
            if model.components_tab == ComponentsTab::Plugins {
                model.modal = Some(Modal::InstallPlugin(InstallPluginModal {
                    url_buffer: String::new(),
                    cursor: 0,
                    error_message: None,
                    installing: false,
                }));
            } else {
                let fe_theme = ExplorerTheme::default()
                    .add_default_title()
                    .with_block(Block::default().borders(Borders::NONE))
                    .with_item_style(Style::default().fg(model.theme.file_text))
                    .with_dir_style(Style::default().fg(model.theme.file_dir))
                    .with_highlight_item_style(
                        Style::default()
                            .fg(model.theme.file_text)
                            .bg(model.theme.file_highlight_bg)
                            .add_modifier(Modifier::BOLD),
                    )
                    .with_highlight_dir_style(
                        Style::default()
                            .fg(model.theme.file_dir)
                            .bg(model.theme.file_highlight_bg)
                            .add_modifier(Modifier::BOLD),
                    )
                    .with_highlight_symbol("▶ ");

                match FileExplorerBuilder::build_with_theme(fe_theme) {
                    Ok(fe) => {
                        model.modal = Some(Modal::Import(Box::new(ImportModal {
                            file_explorer: fe,
                            component_type: model.components_tab,
                            error_message: None,
                        })));
                    }
                    Err(e) => {
                        model.modal = Some(Modal::Error {
                            title: "Import Error".to_string(),
                            message: format!("Failed to open file explorer: {e}"),
                        });
                    }
                }
            }
            Some(vec![])
        }
        KeyCode::Char('g') => {
            if model.components_tab == ComponentsTab::Plugins {
                model.modal = Some(Modal::InstallPlugin(InstallPluginModal {
                    url_buffer: String::new(),
                    cursor: 0,
                    error_message: None,
                    installing: false,
                }));
            } else {
                model.modal = Some(Modal::GitHubImport(GitHubImportModal {
                    url_buffer: String::new(),
                    cursor: 0,
                    component_type: model.components_tab,
                    error_message: None,
                    importing: false,
                }));
            }
            Some(vec![])
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Modal key dispatch
// ---------------------------------------------------------------------------

fn update_modal_key(model: &mut Model, key: ratatui::crossterm::event::KeyEvent) -> Vec<Cmd> {
    let Some(modal) = model.modal.as_mut() else {
        return vec![];
    };

    match modal {
        Modal::MissingEnv { .. } | Modal::Info { .. } | Modal::Error { .. } => {
            if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                model.modal = None;
            }
            vec![]
        }
        Modal::CreateAgent(state) => {
            let (close, cmds) = update_create_agent_modal(state, key);
            if close {
                model.modal = None;
            }
            cmds
        }
        Modal::ConfirmDeleteAgent(state) => {
            if key.code == KeyCode::Esc
                || matches!(key.code, KeyCode::Char('n') | KeyCode::Char('N'))
            {
                model.modal = None;
                return vec![];
            }
            if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) && !state.deleting {
                state.deleting = true;
                return vec![Cmd::DeleteAgent {
                    id: state.agent_id.clone(),
                }];
            }
            vec![]
        }
        Modal::ConfirmDeleteConversation(state) => {
            if key.code == KeyCode::Esc
                || matches!(key.code, KeyCode::Char('n') | KeyCode::Char('N'))
            {
                model.modal = None;
                return vec![];
            }
            if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) && !state.deleting {
                state.deleting = true;
                return vec![Cmd::DeleteConversation {
                    id: state.conversation_id.clone(),
                }];
            }
            vec![]
        }

        Modal::Import(state) => {
            if key.code == KeyCode::Esc {
                model.modal = None;
                return vec![];
            }

            if key.code == KeyCode::Enter {
                let selected = state.file_explorer.current().clone();
                if selected.is_file() {
                    let path = &selected.path;
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("");
                    if ext == "yaml" || ext == "yml" {
                        let path_str = path.to_string_lossy().to_string();
                        let component_type = state.component_type;
                        model.modal = None;
                        return vec![Cmd::ImportComponentFromFile {
                            path: path_str,
                            component_type,
                        }];
                    } else {
                        state.error_message =
                            Some("Only .yaml/.yml files are allowed.".to_string());
                        return vec![];
                    }
                }
            }

            if matches!(key.code, KeyCode::Left | KeyCode::Right) {
                return vec![];
            }

            state.error_message = None;
            let event = ratatui::crossterm::event::Event::Key(key);
            let _ = state.file_explorer.handle(&event);
            vec![]
        }

        Modal::InstallPlugin(state) => {
            if key.code == KeyCode::Esc {
                model.modal = None;
                return vec![];
            }

            if state.installing {
                return vec![];
            }

            match key.code {
                KeyCode::Enter => {
                    let url = state.url_buffer.trim().to_string();
                    if url.is_empty() {
                        state.error_message = Some("URL is required.".to_string());
                    } else {
                        state.installing = true;
                        state.error_message = None;
                        return vec![Cmd::InstallPluginFromUrl { url }];
                    }
                    vec![]
                }
                KeyCode::Char(c) => {
                    state.url_buffer.insert(state.cursor, c);
                    state.cursor += c.len_utf8();
                    state.error_message = None;
                    vec![]
                }
                KeyCode::Backspace => {
                    if state.cursor > 0 {
                        let prev = state.url_buffer[..state.cursor]
                            .chars()
                            .last()
                            .map(|c| c.len_utf8())
                            .unwrap_or(0);
                        state.cursor -= prev;
                        state.url_buffer.remove(state.cursor);
                    }
                    state.error_message = None;
                    vec![]
                }
                KeyCode::Delete => {
                    if state.cursor < state.url_buffer.len() {
                        state.url_buffer.remove(state.cursor);
                    }
                    vec![]
                }
                KeyCode::Left => {
                    if state.cursor > 0 {
                        let prev = state.url_buffer[..state.cursor]
                            .chars()
                            .last()
                            .map(|c| c.len_utf8())
                            .unwrap_or(0);
                        state.cursor -= prev;
                    }
                    vec![]
                }
                KeyCode::Right => {
                    if state.cursor < state.url_buffer.len() {
                        let next = state.url_buffer[state.cursor..]
                            .chars()
                            .next()
                            .map(|c| c.len_utf8())
                            .unwrap_or(0);
                        state.cursor += next;
                    }
                    vec![]
                }
                KeyCode::Home => {
                    state.cursor = 0;
                    vec![]
                }
                KeyCode::End => {
                    state.cursor = state.url_buffer.len();
                    vec![]
                }
                _ => vec![],
            }
        }

        Modal::GitHubImport(state) => {
            if key.code == KeyCode::Esc {
                model.modal = None;
                return vec![];
            }

            if state.importing {
                return vec![];
            }

            match key.code {
                KeyCode::Enter => {
                    let url = state.url_buffer.trim().to_string();
                    if url.is_empty() {
                        state.error_message = Some("URL is required.".to_string());
                    } else {
                        state.importing = true;
                        state.error_message = None;
                        let component_type = state.component_type;
                        return vec![Cmd::ImportComponentFromGitHub { url, component_type }];
                    }
                    vec![]
                }
                KeyCode::Char(c) => {
                    state.url_buffer.insert(state.cursor, c);
                    state.cursor += c.len_utf8();
                    state.error_message = None;
                    vec![]
                }
                KeyCode::Backspace => {
                    if state.cursor > 0 {
                        let prev = state.url_buffer[..state.cursor]
                            .chars()
                            .last()
                            .map(|c| c.len_utf8())
                            .unwrap_or(0);
                        state.cursor -= prev;
                        state.url_buffer.remove(state.cursor);
                    }
                    state.error_message = None;
                    vec![]
                }
                KeyCode::Delete => {
                    if state.cursor < state.url_buffer.len() {
                        state.url_buffer.remove(state.cursor);
                    }
                    vec![]
                }
                KeyCode::Left => {
                    if state.cursor > 0 {
                        let prev = state.url_buffer[..state.cursor]
                            .chars()
                            .last()
                            .map(|c| c.len_utf8())
                            .unwrap_or(0);
                        state.cursor -= prev;
                    }
                    vec![]
                }
                KeyCode::Right => {
                    if state.cursor < state.url_buffer.len() {
                        let next = state.url_buffer[state.cursor..]
                            .chars()
                            .next()
                            .map(|c| c.len_utf8())
                            .unwrap_or(0);
                        state.cursor += next;
                    }
                    vec![]
                }
                KeyCode::Home => {
                    state.cursor = 0;
                    vec![]
                }
                KeyCode::End => {
                    state.cursor = state.url_buffer.len();
                    vec![]
                }
                _ => vec![],
            }
        }

        Modal::ImportAgent(state) => {
            if key.code == KeyCode::Esc {
                model.modal = None;
                return vec![];
            }

            if key.code == KeyCode::Enter {
                let selected = state.file_explorer.current().clone();
                if selected.is_file() {
                    let path = &selected.path;
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("");
                    if ext == "yaml" || ext == "yml" {
                        let path_str = path.to_string_lossy().to_string();
                        model.modal = None;
                        return vec![Cmd::ImportAgentFromFile { path: path_str }];
                    } else {
                        state.error_message =
                            Some("Only .yaml/.yml files are allowed.".to_string());
                        return vec![];
                    }
                }
            }

            if matches!(key.code, KeyCode::Left | KeyCode::Right) {
                return vec![];
            }

            state.error_message = None;
            let event = ratatui::crossterm::event::Event::Key(key);
            let _ = state.file_explorer.handle(&event);
            vec![]
        }

        Modal::GitHubImportAgent(state) => {
            if key.code == KeyCode::Esc {
                model.modal = None;
                return vec![];
            }

            if state.importing {
                return vec![];
            }

            match key.code {
                KeyCode::Enter => {
                    let url = state.url_buffer.trim().to_string();
                    if url.is_empty() {
                        state.error_message = Some("URL is required.".to_string());
                    } else {
                        state.importing = true;
                        state.error_message = None;
                        return vec![Cmd::ImportAgentFromGitHub { url }];
                    }
                    vec![]
                }
                KeyCode::Char(c) => {
                    state.url_buffer.insert(state.cursor, c);
                    state.cursor += c.len_utf8();
                    state.error_message = None;
                    vec![]
                }
                KeyCode::Backspace => {
                    if state.cursor > 0 {
                        let prev = state.url_buffer[..state.cursor]
                            .chars()
                            .last()
                            .map(|c| c.len_utf8())
                            .unwrap_or(0);
                        state.cursor -= prev;
                        state.url_buffer.remove(state.cursor);
                    }
                    state.error_message = None;
                    vec![]
                }
                KeyCode::Delete => {
                    if state.cursor < state.url_buffer.len() {
                        state.url_buffer.remove(state.cursor);
                    }
                    vec![]
                }
                KeyCode::Left => {
                    if state.cursor > 0 {
                        let prev = state.url_buffer[..state.cursor]
                            .chars()
                            .last()
                            .map(|c| c.len_utf8())
                            .unwrap_or(0);
                        state.cursor -= prev;
                    }
                    vec![]
                }
                KeyCode::Right => {
                    if state.cursor < state.url_buffer.len() {
                        let next = state.url_buffer[state.cursor..]
                            .chars()
                            .next()
                            .map(|c| c.len_utf8())
                            .unwrap_or(0);
                        state.cursor += next;
                    }
                    vec![]
                }
                KeyCode::Home => {
                    state.cursor = 0;
                    vec![]
                }
                KeyCode::End => {
                    state.cursor = state.url_buffer.len();
                    vec![]
                }
                _ => vec![],
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Create / Edit agent modal
// ---------------------------------------------------------------------------

fn update_create_agent_modal(
    state: &mut CreateAgentModal,
    key: ratatui::crossterm::event::KeyEvent,
) -> (bool, Vec<Cmd>) {
    if key.code == KeyCode::Esc {
        return (true, vec![]);
    }

    if state.submitting {
        return (false, vec![]);
    }

    const TAB_ORDER: [CreateAgentTab; 4] = [
        CreateAgentTab::Prompt,
        CreateAgentTab::Tools,
        CreateAgentTab::Skills,
        CreateAgentTab::Plugins,
    ];

    match key.code {
        KeyCode::Left => {
            if let Some(pos) = TAB_ORDER.iter().position(|t| *t == state.tab)
                && pos > 0
            {
                state.tab = TAB_ORDER[pos - 1];
                return (false, vec![]);
            }
        }
        KeyCode::Right => {
            if let Some(pos) = TAB_ORDER.iter().position(|t| *t == state.tab)
                && pos + 1 < TAB_ORDER.len()
            {
                state.tab = TAB_ORDER[pos + 1];
                return (false, vec![]);
            }
        }
        _ => {}
    }

    // Ctrl+S submit works from any tab.
    let submit =
        key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s');
    if submit {
        state.error = None;
        let name = state.name.trim().to_string();
        if name.is_empty() {
            state.error = Some("Name is required.".to_string());
            return (false, vec![]);
        }
        let description = state.description.trim().to_string();
        let instructions = state.instructions.trim().to_string();
        if instructions.is_empty() {
            state.error = Some("Instructions are required.".to_string());
            return (false, vec![]);
        }
        if state.selected_tool_ids.is_empty() {
            state.error = Some("Select at least one tool.".to_string());
            return (false, vec![]);
        }

        let (is_edit, id) = match &state.mode {
            AgentEditorMode::Create => (false, generate_agent_id(&name)),
            AgentEditorMode::Edit { agent_id } => (true, agent_id.clone()),
        };

        state.submitting = true;
        return (
            false,
            vec![Cmd::UpsertAgent {
                is_edit,
                id,
                name,
                description,
                instructions,
                tool_ids: state.selected_tool_ids.clone(),
                skill_ids: state.selected_skill_ids.clone(),
                plugin_ids: state.selected_plugin_ids.clone(),
                enable_elastic_capabilities: state.enable_elastic_capabilities,
            }],
        );
    }

    if state.tab == CreateAgentTab::Tools {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if !state.tools.is_empty() {
                    state.tools_selected_index = state.tools_selected_index.saturating_sub(1);
                    state
                        .tools_list_state
                        .select(Some(state.tools_selected_index));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !state.tools.is_empty() {
                    state.tools_selected_index =
                        (state.tools_selected_index + 1).min(state.tools.len() - 1);
                    state
                        .tools_list_state
                        .select(Some(state.tools_selected_index));
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                if !state.tools.is_empty() {
                    let idx = state.tools_selected_index.min(state.tools.len() - 1);
                    let tool_id = state.tools[idx].id.clone();
                    if let Some(pos) = state.selected_tool_ids.iter().position(|id| *id == tool_id)
                    {
                        state.selected_tool_ids.remove(pos);
                    } else {
                        state.selected_tool_ids.push(tool_id);
                    }
                }
            }
            _ => {}
        }
        return (false, vec![]);
    }

    if state.tab == CreateAgentTab::Skills {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if !state.skills.is_empty() {
                    state.skills_selected_index = state.skills_selected_index.saturating_sub(1);
                    state
                        .skills_list_state
                        .select(Some(state.skills_selected_index));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !state.skills.is_empty() {
                    state.skills_selected_index =
                        (state.skills_selected_index + 1).min(state.skills.len() - 1);
                    state
                        .skills_list_state
                        .select(Some(state.skills_selected_index));
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                if !state.skills.is_empty() {
                    let idx = state.skills_selected_index.min(state.skills.len() - 1);
                    let skill_id = state.skills[idx].id.clone();
                    if let Some(pos) =
                        state.selected_skill_ids.iter().position(|id| *id == skill_id)
                    {
                        state.selected_skill_ids.remove(pos);
                    } else {
                        state.selected_skill_ids.push(skill_id);
                    }
                }
            }
            _ => {}
        }
        return (false, vec![]);
    }

    if state.tab == CreateAgentTab::Plugins {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if !state.plugins.is_empty() {
                    state.plugins_selected_index = state.plugins_selected_index.saturating_sub(1);
                    state
                        .plugins_list_state
                        .select(Some(state.plugins_selected_index));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !state.plugins.is_empty() {
                    state.plugins_selected_index =
                        (state.plugins_selected_index + 1).min(state.plugins.len() - 1);
                    state
                        .plugins_list_state
                        .select(Some(state.plugins_selected_index));
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                if !state.plugins.is_empty() {
                    let idx = state.plugins_selected_index.min(state.plugins.len() - 1);
                    let plugin_id = state.plugins[idx].id.clone();
                    if let Some(pos) =
                        state.selected_plugin_ids.iter().position(|id| *id == plugin_id)
                    {
                        state.selected_plugin_ids.remove(pos);
                    } else {
                        state.selected_plugin_ids.push(plugin_id);
                    }
                }
            }
            _ => {}
        }
        return (false, vec![]);
    }

    match key.code {
        KeyCode::Tab => {
            state.focus = match state.focus {
                CreateAgentField::Name => CreateAgentField::Description,
                CreateAgentField::Description => CreateAgentField::Instructions,
                CreateAgentField::Instructions => CreateAgentField::ElasticCapabilities,
                CreateAgentField::ElasticCapabilities => CreateAgentField::Name,
            };
        }
        KeyCode::BackTab => {
            state.focus = match state.focus {
                CreateAgentField::Name => CreateAgentField::ElasticCapabilities,
                CreateAgentField::Description => CreateAgentField::Name,
                CreateAgentField::Instructions => CreateAgentField::Description,
                CreateAgentField::ElasticCapabilities => CreateAgentField::Instructions,
            };
        }
        KeyCode::Enter => {
            if state.focus == CreateAgentField::Instructions {
                state.instructions.push('\n');
            } else if state.focus == CreateAgentField::ElasticCapabilities {
                state.enable_elastic_capabilities = !state.enable_elastic_capabilities;
            } else {
                state.focus = match state.focus {
                    CreateAgentField::Name => CreateAgentField::Description,
                    CreateAgentField::Description => CreateAgentField::Instructions,
                    CreateAgentField::Instructions => CreateAgentField::ElasticCapabilities,
                    CreateAgentField::ElasticCapabilities => CreateAgentField::Name,
                };
            }
        }
        KeyCode::Char(' ') if state.focus == CreateAgentField::ElasticCapabilities => {
            state.enable_elastic_capabilities = !state.enable_elastic_capabilities;
        }
        KeyCode::Backspace => {
            if state.focus != CreateAgentField::ElasticCapabilities {
                let field = focused_field_mut(state);
                field.pop();
            }
        }
        KeyCode::Char(c) => {
            if state.focus == CreateAgentField::ElasticCapabilities {
                return (false, vec![]);
            }
            if key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::ALT)
            {
                return (false, vec![]);
            }
            let field = focused_field_mut(state);
            field.push(c);
        }
        _ => {}
    }

    (false, vec![])
}

fn focused_field_mut(state: &mut CreateAgentModal) -> &mut String {
    match state.focus {
        CreateAgentField::Name => &mut state.name,
        CreateAgentField::Description => &mut state.description,
        CreateAgentField::Instructions | CreateAgentField::ElasticCapabilities => {
            &mut state.instructions
        }
    }
}

fn generate_agent_id(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in name.trim().chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
        if out.len() >= 48 {
            break;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("agent");
    }
    out
}

// ---------------------------------------------------------------------------
// Chat input key handling
// ---------------------------------------------------------------------------

fn handle_chat_input_key(
    model: &mut Model,
    key: ratatui::crossterm::event::KeyEvent,
) -> Option<Vec<Cmd>> {
    if key.modifiers.contains(KeyModifiers::ALT) {
        return None;
    }

    // Scroll keys work even when no session is active.
    match key.code {
        KeyCode::PageUp => {
            scroll_chat(model, ScrollDir::Up, 10);
            return Some(vec![]);
        }
        KeyCode::PageDown => {
            scroll_chat(model, ScrollDir::Down, 10);
            return Some(vec![]);
        }
        KeyCode::Up => {
            scroll_chat(model, ScrollDir::Up, 3);
            return Some(vec![]);
        }
        KeyCode::Down => {
            scroll_chat(model, ScrollDir::Down, 3);
            return Some(vec![]);
        }
        _ => {}
    }

    let session = model.active_session_mut()?;

    match key.code {
        KeyCode::Enter => {
            if session.waiting_for_response {
                return Some(vec![]);
            }
            let text = session.input_buffer.trim().to_string();
            if text.is_empty() {
                return Some(vec![]);
            }
            session.push_chat(ChatEntry {
                role: ChatRole::User,
                content: text.clone(),
                steps: Vec::new(),
            });
            session.input_buffer.clear();
            session.input_cursor = 0;
            session.waiting_for_response = true;
            session.chat_scroll_from_bottom = 0;
            Some(vec![Cmd::SendPrompt { text }])
        }

        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Some(conv_id) = session.conversation_id.clone() {
                session.history_loading = true;
                session.chat_scroll_from_bottom = 0;
                Some(vec![Cmd::LoadConversationHistory {
                    conversation_id: conv_id,
                }])
            } else {
                Some(vec![])
            }
        }

        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                return None;
            }
            let idx = session.input_cursor.min(session.input_buffer.len());
            session.input_buffer.insert(idx, c);
            session.input_cursor = idx + c.len_utf8();
            Some(vec![])
        }

        KeyCode::Backspace => {
            if session.input_cursor > 0 {
                let prev = prev_char_boundary(&session.input_buffer, session.input_cursor);
                session
                    .input_buffer
                    .replace_range(prev..session.input_cursor, "");
                session.input_cursor = prev;
            }
            Some(vec![])
        }

        KeyCode::Delete => {
            if session.input_cursor < session.input_buffer.len() {
                let next = next_char_boundary(&session.input_buffer, session.input_cursor);
                session
                    .input_buffer
                    .replace_range(session.input_cursor..next, "");
            }
            Some(vec![])
        }

        KeyCode::Left => {
            session.input_cursor =
                prev_char_boundary(&session.input_buffer, session.input_cursor);
            Some(vec![])
        }

        KeyCode::Right => {
            session.input_cursor =
                next_char_boundary(&session.input_buffer, session.input_cursor);
            Some(vec![])
        }

        KeyCode::Home => {
            session.input_cursor = 0;
            Some(vec![])
        }

        KeyCode::End => {
            session.input_cursor = session.input_buffer.len();
            Some(vec![])
        }

        KeyCode::Tab | KeyCode::BackTab => None,

        KeyCode::Esc => None,

        _ => Some(vec![]),
    }
}

fn prev_char_boundary(s: &str, idx: usize) -> usize {
    if idx == 0 {
        return 0;
    }
    s[..idx].char_indices().last().map(|(i, _)| i).unwrap_or(0)
}

fn next_char_boundary(s: &str, idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    let mut iter = s[idx..].char_indices();
    let _ = iter.next();
    match iter.next() {
        Some((off, _)) => idx + off,
        None => s.len(),
    }
}

// ---------------------------------------------------------------------------
// Chat scrolling
// ---------------------------------------------------------------------------

enum ScrollDir {
    Up,
    Down,
}

fn scroll_chat(model: &mut Model, dir: ScrollDir, amount: u16) {
    if let Some(session) = model.active_session_mut() {
        match dir {
            ScrollDir::Up => {
                session.chat_scroll_from_bottom =
                    session.chat_scroll_from_bottom.saturating_add(amount);
            }
            ScrollDir::Down => {
                session.chat_scroll_from_bottom =
                    session.chat_scroll_from_bottom.saturating_sub(amount);
            }
        }
    }
}

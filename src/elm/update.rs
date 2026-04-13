use ratatui::crossterm::event::{KeyCode, KeyModifiers, MouseEventKind};
use ratatui::widgets::ListState;

use super::cmd::Cmd;
use super::model::{
    ActivePanel, AgentEditorMode, ChatEntry, ChatRole, ChatSession, ConfirmDeleteAgentModal,
    CreateAgentField, CreateAgentModal, CreateAgentTab, Modal, Model,
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
            model.config = config;
            model.env_loaded = true;
            if !model.config.is_ready() {
                model.modal = Some(Modal::MissingEnv {
                    missing: model.config.missing(),
                });
                return vec![];
            }
            model.agents_loading = true;
            model.components_tools_loading = true;
            model.components_skills_loading = true;
            model.components_plugins_loading = true;
            vec![Cmd::LoadAgents, Cmd::LoadComponentsData]
        }

        // -- Agents --
        Msg::AgentsLoaded { agents } => {
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

        Msg::AgentsLoadFailed { error } => {
            model.agents_loading = false;
            model.agents_error = Some(error);
            vec![]
        }

        // -- Tools (for create/edit modal) --
        Msg::ToolsLoaded { tools } => {
            if let Some(Modal::CreateAgent(state)) = &mut model.modal {
                state.tools_loading = false;
                state.tools_error = None;
                let valid_ids: Vec<String> = tools.iter().map(|t| t.id.clone()).collect();
                state
                    .selected_tool_ids
                    .retain(|id| valid_ids.contains(id));
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
                let valid_ids: Vec<String> = skills.iter().map(|s| s.id.clone()).collect();
                state.selected_skill_ids.retain(|id| valid_ids.contains(id));
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
                let valid_ids: Vec<String> = plugins.iter().map(|p| p.id.clone()).collect();
                state
                    .selected_plugin_ids
                    .retain(|id| valid_ids.contains(id));
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
        } => {
            model.components_tools_loading = false;
            model.components_skills_loading = false;
            model.components_plugins_loading = false;
            model.components_tools_error = None;
            model.components_skills_error = None;
            model.components_plugins_error = None;
            model.components_tools = tools;
            model.components_skills = skills;
            model.components_plugins = plugins;
            vec![]
        }

        Msg::ComponentsDataFailed { error } => {
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
                session.chat.push(ChatEntry {
                    role: ChatRole::System,
                    content: format!(
                        "{} agent: {} ({})",
                        if is_edit { "Updated" } else { "Created" },
                        agent.name,
                        agent.id
                    ),
                });
            }
            if matches!(model.modal, Some(Modal::CreateAgent(_))) {
                model.modal = None;
            }
            model.selected_agent_id = Some(agent.id.clone());
            model.config.agent_id = agent.id;

            model.agents_loaded = false;
            model.agents_loading = false;
            model.agents_error = None;
            model.agents.clear();
            model.agent_selected_index = 0;
            model.agents_list_state = ListState::default();
            model.active = ActivePanel::Agents;
            vec![Cmd::LoadAgents]
        }

        Msg::AgentUpsertFailed { error, is_edit: _ } => {
            if let Some(Modal::CreateAgent(state)) = &mut model.modal {
                state.submitting = false;
                state.error = Some(error);
            }
            vec![]
        }

        Msg::AgentDeleted { id: _, name } => {
            model.modal = None;
            if let Some(session) = model.active_session_mut() {
                session.chat.push(ChatEntry {
                    role: ChatRole::System,
                    content: format!("Deleted agent: {name}"),
                });
            }
            model.agents_loaded = false;
            model.agents_loading = false;
            model.agents.clear();
            model.agent_selected_index = 0;
            model.agents_list_state = ListState::default();
            vec![Cmd::LoadAgents]
        }

        Msg::AgentDeleteFailed { error } => {
            model.modal = Some(Modal::Error {
                title: "Delete failed".to_string(),
                message: error,
            });
            vec![]
        }

        // -- Conversations --
        Msg::ConversationsLoaded { conversations } => {
            model.conversations_loading = false;

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

                let session_id = model.next_session_id;
                model.next_session_id += 1;

                model.sessions.push(ChatSession {
                    id: session_id,
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
                });
            }

            if model.active_session_index.is_none() && !model.sessions.is_empty() {
                model.active_session_index = Some(0);
            }
            sync_sessions_list_state(model);
            vec![]
        }

        Msg::ConversationsLoadFailed { error } => {
            model.conversations_loading = false;
            if let Some(session) = model.active_session_mut() {
                session.chat.push(ChatEntry {
                    role: ChatRole::System,
                    content: format!("Failed to load conversations: {error}"),
                });
            }
            vec![]
        }

        Msg::ConversationHistoryLoaded {
            conversation_id,
            messages,
        } => {
            if let Some(session) = model
                .sessions
                .iter_mut()
                .find(|s| s.conversation_id.as_deref() == Some(&conversation_id))
            {
                session.history_loading = false;
                session.history_loaded = true;
                session.chat.clear();
                for (role, content) in messages {
                    let chat_role = match role.as_str() {
                        "user" | "human" => ChatRole::User,
                        "assistant" | "ai" | "bot" => ChatRole::Assistant,
                        _ => ChatRole::System,
                    };
                    session.chat.push(ChatEntry {
                        role: chat_role,
                        content,
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
                session.chat.push(ChatEntry {
                    role: ChatRole::System,
                    content: format!("Failed to load history: {error}"),
                });
            }
            vec![]
        }

        // -- Chat --
        Msg::PromptResponseReceived {
            content,
            conversation_id,
        } => {
            if let Some(session) = model.active_session_mut() {
                session.waiting_for_response = false;
                session.conversation_id = conversation_id.or(session.conversation_id.take());
                session.chat.push(ChatEntry {
                    role: ChatRole::Assistant,
                    content,
                });
                session.chat_scroll_from_bottom = 0;
            }
            vec![]
        }

        Msg::PromptResponseFailed { error } => {
            if let Some(session) = model.active_session_mut() {
                session.waiting_for_response = false;
                session.chat.push(ChatEntry {
                    role: ChatRole::Assistant,
                    content: format!("Error: {error}"),
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

                let session_id = model.next_session_id;
                model.next_session_id += 1;
                let session = ChatSession {
                    id: session_id,
                    agent_id: agent_id.clone(),
                    agent_name: agent_name.clone(),
                    title: "New chat".to_string(),
                    conversation_id: None,
                    chat: vec![ChatEntry {
                        role: ChatRole::System,
                        content: format!("New chat with {agent_name} ({agent_id})"),
                    }],
                    input_buffer: String::new(),
                    input_cursor: 0,
                    waiting_for_response: false,
                    chat_scroll_from_bottom: 0,
                    from_server: false,
                    history_loaded: true,
                    history_loading: false,
                };
                model.sessions.push(session);
                let new_idx = model.sessions.len() - 1;
                model.active_session_index = Some(new_idx);
                sync_sessions_list_state(model);

                model.selected_agent_id = Some(agent_id.clone());
                model.config.agent_id = agent_id;
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
        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            model.agents_loading = true;
            model.agents_loaded = false;
            model.agents_error = None;
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
        KeyCode::Char('x') => {
            if let Some(active_idx) = model.active_session_index
                && active_idx < model.sessions.len()
            {
                model.sessions.remove(active_idx);

                // Recompute filtered indices after removal.
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
            }
            Some(vec![])
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
    let ids = model
        .active_session()
        .map(|s| s.agent_id.clone());
    if let Some(agent_id) = ids {
        model.selected_agent_id = Some(agent_id.clone());
        model.config.agent_id = agent_id;
    }
}

// ---------------------------------------------------------------------------
// Components panel key handling
// ---------------------------------------------------------------------------

fn handle_components_panel_key(
    model: &mut Model,
    key: ratatui::crossterm::event::KeyEvent,
) -> Option<Vec<Cmd>> {
    use super::model::ComponentsTab;

    const TAB_ORDER: [ComponentsTab; 3] = [
        ComponentsTab::Plugins,
        ComponentsTab::Skills,
        ComponentsTab::Tools,
    ];

    match key.code {
        KeyCode::Left | KeyCode::Char('h') => {
            if let Some(pos) = TAB_ORDER.iter().position(|t| *t == model.components_tab)
                && pos > 0
            {
                model.components_tab = TAB_ORDER[pos - 1];
            }
            Some(vec![])
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if let Some(pos) = TAB_ORDER.iter().position(|t| *t == model.components_tab)
                && pos + 1 < TAB_ORDER.len()
            {
                model.components_tab = TAB_ORDER[pos + 1];
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
                state.tab = TAB_ORDER[pos - 1].clone();
                return (false, vec![]);
            }
        }
        KeyCode::Right => {
            if let Some(pos) = TAB_ORDER.iter().position(|t| *t == state.tab)
                && pos + 1 < TAB_ORDER.len()
            {
                state.tab = TAB_ORDER[pos + 1].clone();
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
                CreateAgentField::Instructions => CreateAgentField::Name,
            };
        }
        KeyCode::BackTab => {
            state.focus = match state.focus {
                CreateAgentField::Name => CreateAgentField::Instructions,
                CreateAgentField::Description => CreateAgentField::Name,
                CreateAgentField::Instructions => CreateAgentField::Description,
            };
        }
        KeyCode::Enter => {
            if state.focus == CreateAgentField::Instructions {
                state.instructions.push('\n');
            } else {
                state.focus = match state.focus {
                    CreateAgentField::Name => CreateAgentField::Description,
                    CreateAgentField::Description => CreateAgentField::Instructions,
                    CreateAgentField::Instructions => CreateAgentField::Name,
                };
            }
        }
        KeyCode::Backspace => {
            let field = focused_field_mut(state);
            field.pop();
        }
        KeyCode::Char(c) => {
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
        CreateAgentField::Instructions => &mut state.instructions,
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
            let text = session.input_buffer.trim().to_string();
            if text.is_empty() {
                return Some(vec![]);
            }
            session.chat.push(ChatEntry {
                role: ChatRole::User,
                content: text.clone(),
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

use std::sync::Arc;

use ratatui::widgets::ListState;
use ratatui_explorer::FileExplorer;

use crate::agent_builder::{AgentSummary, PluginSummary, SkillSummary, ToolSummary};
use crate::config::Config;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ActivePanel {
    #[default]
    Agents,
    Chats,
    Chat,
    Components,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ComponentsTab {
    #[default]
    Plugins,
    Skills,
    Tools,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone)]
pub struct ChatEntry {
    pub role: ChatRole,
    pub content: String,
}

pub const MAX_CHAT_MESSAGES: usize = 2000;
pub const MAX_SESSIONS: usize = 200;

/// A single chat session tied to a specific agent.
#[derive(Debug, Clone)]
pub struct ChatSession {
    pub agent_id: String,
    pub agent_name: String,
    pub title: String,
    pub conversation_id: Option<String>,
    pub chat: Vec<ChatEntry>,
    pub input_buffer: String,
    pub input_cursor: usize,
    pub waiting_for_response: bool,
    /// Lines scrolled up from the bottom. `0` means pinned to latest messages.
    pub chat_scroll_from_bottom: u16,
    /// True if this session was loaded from the server (pre-existing conversation).
    pub from_server: bool,
    /// True once full message history has been fetched for a server-side conversation.
    pub history_loaded: bool,
    /// True while conversation history is being fetched.
    pub history_loading: bool,
}

#[derive(Debug, Default)]
pub struct Model {
    pub should_quit: bool,
    pub active: ActivePanel,

    // -- Config --
    pub config: Arc<Config>,
    pub env_loaded: bool,

    // -- Chat sessions --
    pub sessions: Vec<ChatSession>,
    pub active_session_index: Option<usize>,
    pub sessions_list_state: ListState,

    // -- Conversations --
    pub conversations_loading: bool,

    // -- Agents --
    pub agents_loading: bool,
    pub agents_loaded: bool,
    pub agents_error: Option<String>,
    pub agents: Vec<AgentSummary>,
    pub agent_selected_index: usize,
    pub selected_agent_id: Option<String>,
    pub agents_list_state: ListState,
    /// Monotonic counter incremented each time an agents load is requested.
    pub agents_generation: u64,

    // -- Components panel --
    pub components_tab: ComponentsTab,
    pub components_tools: Vec<ToolSummary>,
    pub components_tools_loading: bool,
    pub components_tools_error: Option<String>,
    pub components_skills: Vec<SkillSummary>,
    pub components_skills_loading: bool,
    pub components_skills_error: Option<String>,
    pub components_plugins: Vec<PluginSummary>,
    pub components_plugins_loading: bool,
    pub components_plugins_error: Option<String>,
    /// Monotonic counter incremented each time a components load is requested.
    pub components_generation: u64,

    // -- Modal --
    pub modal: Option<Modal>,
}

impl ChatSession {
    /// Push a chat entry, evicting the oldest messages if over the cap.
    pub fn push_chat(&mut self, entry: ChatEntry) {
        if self.chat.len() >= MAX_CHAT_MESSAGES {
            let drain_count = self.chat.len() - MAX_CHAT_MESSAGES + 1;
            self.chat.drain(..drain_count);
        }
        self.chat.push(entry);
    }
}

impl Model {
    /// Returns the currently active chat session, if any.
    pub fn active_session(&self) -> Option<&ChatSession> {
        self.active_session_index
            .and_then(|idx| self.sessions.get(idx))
    }

    /// Returns the currently active chat session mutably, if any.
    pub fn active_session_mut(&mut self) -> Option<&mut ChatSession> {
        self.active_session_index
            .and_then(|idx| self.sessions.get_mut(idx))
    }

    /// Evict the oldest non-active sessions to stay within `MAX_SESSIONS`.
    pub fn enforce_session_cap(&mut self) {
        while self.sessions.len() > MAX_SESSIONS {
            let remove_idx = self
                .sessions
                .iter()
                .enumerate()
                .find(|(i, _)| Some(*i) != self.active_session_index)
                .map(|(i, _)| i);
            if let Some(idx) = remove_idx {
                self.sessions.remove(idx);
                if let Some(active) = self.active_session_index {
                    if active > idx {
                        self.active_session_index = Some(active - 1);
                    }
                }
            } else {
                break;
            }
        }
    }
}

// -- Modals --

pub enum Modal {
    MissingEnv { missing: Vec<&'static str> },
    Info { title: String, message: String },
    Error { title: String, message: String },
    CreateAgent(Box<CreateAgentModal>),
    ConfirmDeleteAgent(ConfirmDeleteAgentModal),
    Import(Box<ImportModal>),
}

impl std::fmt::Debug for Modal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingEnv { missing } => f.debug_struct("MissingEnv").field("missing", missing).finish(),
            Self::Info { title, .. } => f.debug_struct("Info").field("title", title).finish(),
            Self::Error { title, .. } => f.debug_struct("Error").field("title", title).finish(),
            Self::CreateAgent(_) => f.debug_tuple("CreateAgent").finish(),
            Self::ConfirmDeleteAgent(s) => f.debug_tuple("ConfirmDeleteAgent").field(s).finish(),
            Self::Import(_) => f.debug_tuple("Import").finish(),
        }
    }
}

pub struct ImportModal {
    pub file_explorer: FileExplorer,
    pub component_type: ComponentsTab,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ConfirmDeleteAgentModal {
    pub agent_id: String,
    pub agent_name: String,
    pub deleting: bool,
}

#[derive(Debug, Clone)]
pub enum AgentEditorMode {
    Create,
    Edit { agent_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateAgentTab {
    Prompt,
    Tools,
    Skills,
    Plugins,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateAgentField {
    Name,
    Description,
    Instructions,
}

#[derive(Debug, Clone)]
pub struct CreateAgentModal {
    pub mode: AgentEditorMode,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub focus: CreateAgentField,
    pub tab: CreateAgentTab,

    pub tools_loading: bool,
    pub tools_error: Option<String>,
    pub tools: Vec<ToolSummary>,
    pub tools_selected_index: usize,
    pub tools_list_state: ListState,
    pub selected_tool_ids: Vec<String>,

    pub skills_loading: bool,
    pub skills_error: Option<String>,
    pub skills: Vec<SkillSummary>,
    pub skills_selected_index: usize,
    pub skills_list_state: ListState,
    pub selected_skill_ids: Vec<String>,

    pub plugins_loading: bool,
    pub plugins_error: Option<String>,
    pub plugins: Vec<PluginSummary>,
    pub plugins_selected_index: usize,
    pub plugins_list_state: ListState,
    pub selected_plugin_ids: Vec<String>,

    pub submitting: bool,
    pub error: Option<String>,
}

impl Default for CreateAgentModal {
    fn default() -> Self {
        let selected_tool_ids = vec![
            "platform.core.search".to_string(),
            "platform.core.list_indices".to_string(),
            "platform.core.get_index_mapping".to_string(),
            "platform.core.get_document_by_id".to_string(),
        ];
        Self {
            mode: AgentEditorMode::Create,
            name: String::new(),
            description: String::new(),
            instructions: String::new(),
            focus: CreateAgentField::Name,
            tab: CreateAgentTab::Prompt,
            tools_loading: false,
            tools_error: None,
            tools: Vec::new(),
            tools_selected_index: 0,
            tools_list_state: ListState::default(),
            selected_tool_ids,
            skills_loading: false,
            skills_error: None,
            skills: Vec::new(),
            skills_selected_index: 0,
            skills_list_state: ListState::default(),
            selected_skill_ids: Vec::new(),
            plugins_loading: false,
            plugins_error: None,
            plugins: Vec::new(),
            plugins_selected_index: 0,
            plugins_list_state: ListState::default(),
            selected_plugin_ids: Vec::new(),
            submitting: false,
            error: None,
        }
    }
}

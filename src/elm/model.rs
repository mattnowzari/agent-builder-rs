use ratatui::widgets::ListState;

use crate::agentbuilder::{AgentSummary, ToolSummary};
use crate::config::Config;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ActivePanel {
    #[default]
    Agents,
    Chats,
    Chat,
    Details,
}

#[derive(Debug, Clone)]
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

/// A single chat session tied to a specific agent.
#[derive(Debug, Clone)]
pub struct ChatSession {
    pub id: usize,
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
    pub config: Config,
    pub env_loaded: bool,

    // -- Chat sessions --
    pub sessions: Vec<ChatSession>,
    pub active_session_index: Option<usize>,
    pub sessions_list_state: ListState,
    pub next_session_id: usize,

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

    // -- Modal --
    pub modal: Option<Modal>,
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
}

// -- Modals --

#[derive(Debug, Clone)]
pub enum Modal {
    MissingEnv { missing: Vec<&'static str> },
    Info { title: String, message: String },
    Error { title: String, message: String },
    CreateAgent(CreateAgentModal),
    ConfirmDeleteAgent(ConfirmDeleteAgentModal),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateAgentTab {
    Prompt,
    Tools,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
            submitting: false,
            error: None,
        }
    }
}

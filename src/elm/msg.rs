use ratatui::crossterm::event::{KeyEvent, MouseEvent};

use crate::agentbuilder::{AgentSummary, ConversationSummary, ToolSummary};
use crate::config::Config;

#[derive(Debug, Clone)]
pub enum Msg {
    Init,
    Tick,
    Quit,
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize,

    // -- Config --
    EnvLoaded { config: Config },

    // -- Agents --
    AgentsLoaded { agents: Vec<AgentSummary> },
    AgentsLoadFailed { error: String },

    // -- Conversations --
    ConversationsLoaded { conversations: Vec<ConversationSummary> },
    ConversationsLoadFailed { error: String },
    ConversationHistoryLoaded {
        conversation_id: String,
        messages: Vec<(String, String)>,
    },
    ConversationHistoryFailed {
        conversation_id: String,
        error: String,
    },

    // -- Tools --
    ToolsLoaded { tools: Vec<ToolSummary> },
    ToolsLoadFailed { error: String },

    // -- Agent CRUD --
    AgentUpserted { agent: AgentSummary, is_edit: bool },
    AgentUpsertFailed { error: String, is_edit: bool },
    AgentDeleted { id: String, name: String },
    AgentDeleteFailed { error: String },

    // -- Chat --
    PromptResponseReceived { content: String, conversation_id: Option<String> },
    PromptResponseFailed { error: String },
}

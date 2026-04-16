use ratatui::crossterm::event::{KeyEvent, MouseEvent};

use crate::agent_builder::{AgentSummary, ConversationSummary, PluginSummary, SkillSummary, ToolStep, ToolSummary};
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
    AgentsLoaded { agents: Vec<AgentSummary>, generation: u64 },
    AgentsLoadFailed { error: String, generation: u64 },

    // -- Conversations --
    ConversationsLoaded { conversations: Vec<ConversationSummary> },
    ConversationsLoadFailed { error: String },
    ConversationHistoryLoaded {
        conversation_id: String,
        messages: Vec<(String, String, Vec<ToolStep>)>,
        model_name: Option<String>,
    },
    ConversationHistoryFailed {
        conversation_id: String,
        error: String,
    },

    // -- Tools / Skills / Plugins (modal) --
    ToolsLoaded { tools: Vec<ToolSummary> },
    ToolsLoadFailed { error: String },
    SkillsLoaded { skills: Vec<SkillSummary> },
    SkillsLoadFailed { error: String },
    PluginsLoaded { plugins: Vec<PluginSummary> },
    PluginsLoadFailed { error: String },

    // -- Components panel data --
    ComponentsDataLoaded {
        tools: Vec<ToolSummary>,
        skills: Vec<SkillSummary>,
        plugins: Vec<PluginSummary>,
        generation: u64,
    },
    ComponentsDataFailed { error: String, generation: u64 },

    // -- Agent CRUD --
    AgentUpserted { agent: AgentSummary, is_edit: bool },
    AgentUpsertFailed { error: String },
    AgentDeleted { name: String },
    AgentDeleteFailed { error: String },
    ConversationDeleted { conversation_id: String },
    ConversationDeleteFailed { error: String },

    // -- Import from file --
    ToolCreatedFromFile { tool: ToolSummary },
    ToolCreateFromFileFailed { error: String },
    SkillCreatedFromFile { skill: SkillSummary },
    SkillCreateFromFileFailed { error: String },
    PluginInstalledFromFile { plugin: PluginSummary },
    PluginInstallFromFileFailed { error: String },

    // -- Chat --
    PromptResponseReceived { content: String, conversation_id: Option<String>, model_name: Option<String>, steps: Vec<ToolStep> },
    PromptResponseFailed { error: String },
}

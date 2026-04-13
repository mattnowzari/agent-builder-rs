#[derive(Debug, Clone)]
pub enum Cmd {
    LoadEnv,
    LoadAgents,
    LoadConversations,
    LoadConversationHistory { conversation_id: String },
    LoadTools,
    SendPrompt { text: String },
    UpsertAgent {
        is_edit: bool,
        id: String,
        name: String,
        description: String,
        instructions: String,
        tool_ids: Vec<String>,
    },
    DeleteAgent { id: String },
}

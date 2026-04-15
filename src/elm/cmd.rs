use super::model::ComponentsTab;

#[derive(Debug, Clone)]
pub enum Cmd {
    LoadEnv,
    LoadAgents,
    LoadConversations,
    LoadConversationHistory { conversation_id: String },
    LoadTools,
    LoadSkills,
    LoadPlugins,
    LoadComponentsData,
    SendPrompt { text: String },
    UpsertAgent {
        is_edit: bool,
        id: String,
        name: String,
        description: String,
        instructions: String,
        tool_ids: Vec<String>,
        skill_ids: Vec<String>,
        plugin_ids: Vec<String>,
        enable_elastic_capabilities: bool,
    },
    DeleteAgent { id: String },
    ImportComponentFromFile {
        path: String,
        component_type: ComponentsTab,
    },
}

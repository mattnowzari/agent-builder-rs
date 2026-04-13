use std::collections::HashMap;

use anyhow::{Context, Result};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Debug, Clone)]
pub struct ConversationSummary {
    pub id: String,
    pub agent_id: Option<String>,
    pub title: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ConversationDetail {
    pub summary: ConversationSummary,
    pub messages: Vec<ConversationMessage>,
    /// Raw JSON response for diagnostic purposes.
    pub raw_response: String,
}

#[derive(Debug, Clone)]
pub struct AgentSummary {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub instructions: Option<String>,
    pub tool_ids: Vec<String>,
    pub skill_ids: Vec<String>,
    pub plugin_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolSummary {
    pub id: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(rename = "type", default)]
    pub tool_type: String,
    #[serde(default)]
    pub readonly: bool,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkillSummary {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub readonly: bool,
    #[serde(default)]
    pub plugin_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PluginSummary {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub readonly: bool,
    #[serde(default)]
    pub skill_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AgentBuilderClient {
    base_url: String,
    agent_id: String,
    space: Option<String>,
    http: reqwest::Client,
}

impl AgentBuilderClient {
    pub fn new(cfg: &Config) -> Result<Self> {
        let base_url = normalize_base_url(
            cfg.kibana_url
                .as_deref()
                .context("KIBANA_URL (or ES_HOST) is required")?,
        );
        let api_key = cfg
            .api_key
            .as_deref()
            .context("API_KEY (or ES_API_KEY) is required")?
            .to_string();

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert("kbn-xsrf", HeaderValue::from_static("true"));

        let auth_value = format!("ApiKey {api_key}");
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_value).context("invalid API_KEY value")?,
        );

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .danger_accept_invalid_certs(cfg.insecure_tls)
            .danger_accept_invalid_hostnames(cfg.insecure_tls)
            .build()?;

        Ok(Self {
            base_url,
            agent_id: cfg.agent_id.clone(),
            space: cfg.space.clone(),
            http,
        })
    }

    fn api_url(&self, path: &str) -> String {
        match self.space.as_deref() {
            Some(space) => format!("{}/s/{}/api/agent_builder/{}", self.base_url, space, path),
            None => format!("{}/api/agent_builder/{}", self.base_url, path),
        }
    }

    pub async fn converse(
        &self,
        input: &str,
        conversation_id: Option<&str>,
    ) -> Result<ConverseResult> {
        let url = self.api_url("converse");

        let body = ConverseRequest {
            input,
            agent_id: &self.agent_id,
            conversation_id,
        };

        let resp = self
            .http
            .post(url)
            .json(&body)
            .send()
            .await
            .context("failed to send request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Agent Builder API error {status}: {text}");
        }

        let parsed: ConverseResponse =
            serde_json::from_str(&text).context("failed to parse Agent Builder response JSON")?;

        Ok(ConverseResult {
            conversation_id: parsed.conversation_id,
            message: parsed.response.message,
        })
    }

    pub async fn list_agents(&self) -> Result<Vec<AgentSummary>> {
        let url = self.api_url("agents");
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .context("failed to send request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Agent Builder API error {status}: {text}");
        }

        let v: serde_json::Value =
            serde_json::from_str(&text).context("failed to parse list agents response JSON")?;
        parse_agents(v).context("failed to parse agents from response")
    }

    pub async fn list_tools(&self) -> Result<Vec<ToolSummary>> {
        let url = self.api_url("tools");
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .context("failed to send request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Agent Builder API error {status}: {text}");
        }

        let parsed: ListToolsResponse =
            serde_json::from_str(&text).context("failed to parse list tools response JSON")?;
        Ok(parsed.results)
    }

    pub async fn create_agent(&self, req: CreateAgentRequest) -> Result<AgentSummary> {
        let sent_instructions = req.configuration.instructions.clone();
        let sent_tool_ids = req
            .configuration
            .tools
            .first()
            .map(|t| t.tool_ids.clone())
            .unwrap_or_default();
        let sent_skill_ids = req.configuration.skill_ids.clone();
        let sent_plugin_ids = req.configuration.plugin_ids.clone();

        let url = self.api_url("agents");
        let resp = self
            .http
            .post(url)
            .json(&req)
            .send()
            .await
            .context("failed to send request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Agent Builder API error {status}: {text}");
        }

        let parsed: CreateAgentResponse =
            serde_json::from_str(&text).context("failed to parse create agent response JSON")?;

        Ok(AgentSummary {
            id: parsed.id,
            name: parsed.name,
            description: Some(parsed.description),
            instructions: sent_instructions,
            tool_ids: sent_tool_ids,
            skill_ids: sent_skill_ids,
            plugin_ids: sent_plugin_ids,
        })
    }

    pub async fn update_agent(&self, id: &str, req: UpdateAgentRequest) -> Result<AgentSummary> {
        let sent_instructions = req.configuration.instructions.clone();
        let sent_tool_ids = req
            .configuration
            .tools
            .first()
            .map(|t| t.tool_ids.clone())
            .unwrap_or_default();
        let sent_skill_ids = req.configuration.skill_ids.clone();
        let sent_plugin_ids = req.configuration.plugin_ids.clone();

        let url = self.api_url(&format!("agents/{id}"));
        let resp = self
            .http
            .put(url)
            .json(&req)
            .send()
            .await
            .context("failed to send request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Agent Builder API error {status}: {text}");
        }

        let parsed: CreateAgentResponse =
            serde_json::from_str(&text).context("failed to parse update agent response JSON")?;

        Ok(AgentSummary {
            id: parsed.id,
            name: parsed.name,
            description: Some(parsed.description),
            instructions: sent_instructions,
            tool_ids: sent_tool_ids,
            skill_ids: sent_skill_ids,
            plugin_ids: sent_plugin_ids,
        })
    }

    pub async fn list_skills(&self) -> Result<Vec<SkillSummary>> {
        let url = format!("{}?include_plugins=true", self.api_url("skills"));
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .context("failed to send request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Agent Builder API error {status}: {text}");
        }

        let parsed: ListSkillsResponse =
            serde_json::from_str(&text).context("failed to parse list skills response JSON")?;
        Ok(parsed.results)
    }

    pub async fn list_plugins(&self) -> Result<Vec<PluginSummary>> {
        let url = self.api_url("plugins");
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .context("failed to send request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Agent Builder API error {status}: {text}");
        }

        let parsed: ListPluginsResponse =
            serde_json::from_str(&text).context("failed to parse list plugins response JSON")?;
        Ok(parsed.results)
    }

    pub async fn list_conversations(&self) -> Result<Vec<ConversationSummary>> {
        let url = self.api_url("conversations");
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .context("failed to send request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Agent Builder API error {status}: {text}");
        }

        let v: serde_json::Value = serde_json::from_str(&text)
            .context("failed to parse list conversations response JSON")?;
        parse_conversations(v).context("failed to parse conversations from response")
    }

    pub async fn get_conversation(&self, id: &str) -> Result<ConversationDetail> {
        let url = self.api_url(&format!("conversations/{id}"));
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .context("failed to send request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Agent Builder API error {status}: {text}");
        }

        let v: serde_json::Value = serde_json::from_str(&text)
            .context("failed to parse get conversation response JSON")?;
        parse_conversation_detail(v, text).context("failed to parse conversation detail")
    }

    pub async fn create_tool(&self, req: &CreateToolRequest) -> Result<ToolSummary> {
        let url = self.api_url("tools");
        let resp = self
            .http
            .post(url)
            .json(req)
            .send()
            .await
            .context("failed to send create tool request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Agent Builder API error {status}: {text}");
        }

        let tool: ToolSummary =
            serde_json::from_str(&text).context("failed to parse create tool response")?;
        Ok(tool)
    }

    pub async fn delete_agent(&self, id: &str) -> Result<()> {
        let url = self.api_url(&format!("agents/{id}"));
        let resp = self
            .http
            .delete(url)
            .send()
            .await
            .context("failed to send request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Agent Builder API error {status}: {text}");
        }
        Ok(())
    }
}

fn normalize_base_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

fn parse_agents(v: serde_json::Value) -> Result<Vec<AgentSummary>> {
    fn as_arr(v: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
        v.as_array()
    }

    let arr = if let Some(a) = as_arr(&v) {
        a
    } else if let Some(a) = v.get("agents").and_then(as_arr) {
        a
    } else if let Some(a) = v.get("data").and_then(as_arr) {
        a
    } else if let Some(a) = v.get("items").and_then(as_arr) {
        a
    } else if let Some(a) = v.get("results").and_then(as_arr) {
        a
    } else {
        anyhow::bail!("unexpected list agents JSON shape: {v}");
    };

    let mut out = Vec::new();
    for item in arr {
        let obj = item.as_object().context("agent item is not an object")?;

        let id = obj
            .get("id")
            .or_else(|| obj.get("agent_id"))
            .or_else(|| obj.get("agentId"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if id.trim().is_empty() {
            continue;
        }

        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&id)
            .to_string();
        let description = obj
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let instructions = obj
            .get("configuration")
            .and_then(|v| v.get("instructions"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let config = obj.get("configuration");

        let tool_ids = config
            .and_then(|v| v.get("tools"))
            .and_then(|v| v.as_array())
            .and_then(|tools| tools.first())
            .and_then(|v| v.get("tool_ids"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str())
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

        let skill_ids = config
            .and_then(|v| v.get("skill_ids"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str())
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

        let plugin_ids = config
            .and_then(|v| v.get("plugin_ids"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str())
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

        out.push(AgentSummary {
            id,
            name,
            description,
            instructions,
            tool_ids,
            skill_ids,
            plugin_ids,
        });
    }

    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

fn parse_conversations(v: serde_json::Value) -> Result<Vec<ConversationSummary>> {
    fn as_arr(v: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
        v.as_array()
    }

    let arr = if let Some(a) = as_arr(&v) {
        a
    } else if let Some(a) = v.get("conversations").and_then(as_arr) {
        a
    } else if let Some(a) = v.get("data").and_then(as_arr) {
        a
    } else if let Some(a) = v.get("items").and_then(as_arr) {
        a
    } else if let Some(a) = v.get("results").and_then(as_arr) {
        a
    } else {
        anyhow::bail!("unexpected list conversations JSON shape: {v}");
    };

    let mut out = Vec::new();
    for item in arr {
        let obj = item
            .as_object()
            .context("conversation item is not an object")?;

        let id = obj
            .get("id")
            .or_else(|| obj.get("conversation_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if id.trim().is_empty() {
            continue;
        }

        let agent_id = obj
            .get("agent_id")
            .or_else(|| obj.get("agentId"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let title = obj
            .get("title")
            .or_else(|| obj.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let updated_at = obj
            .get("updated_at")
            .or_else(|| obj.get("updatedAt"))
            .or_else(|| obj.get("created_at"))
            .or_else(|| obj.get("createdAt"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        out.push(ConversationSummary {
            id,
            agent_id,
            title,
            updated_at,
        });
    }

    out.sort_by(|a, b| {
        b.updated_at
            .as_deref()
            .unwrap_or("")
            .cmp(a.updated_at.as_deref().unwrap_or(""))
    });
    Ok(out)
}

fn parse_conversation_detail(v: serde_json::Value, raw: String) -> Result<ConversationDetail> {
    let obj = v.as_object().context("conversation detail is not an object")?;

    let id = obj
        .get("id")
        .or_else(|| obj.get("conversation_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let agent_id = obj
        .get("agent_id")
        .or_else(|| obj.get("agentId"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let title = obj
        .get("title")
        .or_else(|| obj.get("name"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let updated_at = obj
        .get("updated_at")
        .or_else(|| obj.get("updatedAt"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let summary = ConversationSummary {
        id,
        agent_id,
        title,
        updated_at,
    };

    let mut messages = Vec::new();

    // Format 1: flat "messages" array with { role, content } objects
    if let Some(arr) = obj.get("messages").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(msg_obj) = item.as_object() {
                let role = msg_obj
                    .get("role")
                    .or_else(|| msg_obj.get("type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let content = msg_obj
                    .get("content")
                    .and_then(|v| v.as_str())
                    .or_else(|| msg_obj.get("message").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string();

                if !content.is_empty() {
                    messages.push(ConversationMessage { role, content });
                }
            }
        }
    }

    // Format 2: "rounds" array where each round has { input, response { message } }
    // This mirrors the converse API response structure.
    if messages.is_empty()
        && let Some(arr) = obj.get("rounds").and_then(|v| v.as_array())
    {
        for round in arr {
            if let Some(round_obj) = round.as_object() {
                // User input: can be { "message": "..." } object or a plain string
                let input = round_obj
                    .get("input")
                    .and_then(|v| {
                        v.get("message")
                            .and_then(|m| m.as_str())
                            .or_else(|| v.as_str())
                    })
                    .unwrap_or("")
                    .to_string();

                if !input.is_empty() {
                    messages.push(ConversationMessage {
                        role: "user".to_string(),
                        content: input,
                    });
                }

                // Agent response
                let response = round_obj
                    .get("response")
                    .and_then(|v| v.get("message"))
                    .and_then(|v| v.as_str())
                    .or_else(|| round_obj.get("output").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string();

                if !response.is_empty() {
                    messages.push(ConversationMessage {
                        role: "assistant".to_string(),
                        content: response,
                    });
                }
            }
        }
    }

    // Format 3: top-level "events" or "history" array (fallback)
    if messages.is_empty()
        && let Some(arr) = obj
            .get("events")
            .and_then(|v| v.as_array())
            .or_else(|| obj.get("history").and_then(|v| v.as_array()))
    {
        for item in arr {
            if let Some(ev) = item.as_object() {
                let role = ev
                    .get("role")
                    .or_else(|| ev.get("type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let content = ev
                    .get("content")
                    .and_then(|v| v.as_str())
                    .or_else(|| ev.get("message").and_then(|v| v.as_str()))
                    .or_else(|| {
                        ev.get("response")
                            .and_then(|v| v.get("message"))
                            .and_then(|v| v.as_str())
                    })
                    .unwrap_or("")
                    .to_string();

                if !content.is_empty() {
                    messages.push(ConversationMessage { role, content });
                }
            }
        }
    }

    // When no messages were parsed, include the response keys as a diagnostic hint
    // so the user can see what the API actually returned.
    if messages.is_empty() {
        let keys: Vec<&String> = obj.keys().collect();
        messages.push(ConversationMessage {
            role: "system".to_string(),
            content: format!(
                "Could not parse messages from conversation response. Top-level keys: {keys:?}"
            ),
        });
    }

    Ok(ConversationDetail {
        summary,
        messages,
        raw_response: raw,
    })
}

// --- Request / Response types ---

#[derive(Debug, Serialize)]
struct ConverseRequest<'a> {
    input: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    agent_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    conversation_id: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct ConverseResponse {
    response: ConverseResponseMessage,
    conversation_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConverseResponseMessage {
    message: String,
}

#[derive(Debug, Clone)]
pub struct ConverseResult {
    pub conversation_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct ListToolsResponse {
    #[serde(default)]
    results: Vec<ToolSummary>,
}

#[derive(Debug, Deserialize)]
struct ListSkillsResponse {
    #[serde(default)]
    results: Vec<SkillSummary>,
}

#[derive(Debug, Deserialize)]
struct ListPluginsResponse {
    #[serde(default)]
    results: Vec<PluginSummary>,
}

#[derive(Debug, Serialize)]
pub struct CreateAgentRequest {
    pub id: String,
    pub name: String,
    pub description: String,
    pub configuration: AgentConfiguration,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_symbol: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub labels: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateAgentRequest {
    pub name: String,
    pub description: String,
    pub configuration: AgentConfiguration,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_symbol: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub labels: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct AgentConfiguration {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    pub tools: Vec<AgentTools>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub skill_ids: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub plugin_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct AgentTools {
    pub tool_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CreateAgentResponse {
    id: String,
    name: String,
    description: String,
}

// ---------------------------------------------------------------------------
// YAML tool loading
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ToolYaml {
    id: String,
    #[serde(rename = "type")]
    tool_type: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(flatten)]
    extra: HashMap<String, serde_yaml::Value>,
}

#[derive(Debug, Deserialize)]
struct EsqlParamYaml {
    #[serde(rename = "type")]
    param_type: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    optional: bool,
    #[serde(default)]
    default_value: Option<serde_yaml::Value>,
}

#[derive(Debug, Serialize)]
pub struct CreateToolRequest {
    pub id: String,
    #[serde(rename = "type")]
    pub tool_type: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub configuration: serde_json::Value,
}

pub fn parse_tool_yaml(contents: &str) -> Result<CreateToolRequest> {
    let yaml: ToolYaml =
        serde_yaml::from_str(contents).context("failed to parse YAML tool definition")?;

    let configuration = match yaml.tool_type.as_str() {
        "esql" => build_esql_config(&yaml)?,
        "index_search" => build_index_search_config(&yaml)?,
        "workflow" => build_workflow_config(&yaml)?,
        other => anyhow::bail!("unsupported tool type: {other} (expected esql, index_search, or workflow)"),
    };

    Ok(CreateToolRequest {
        id: yaml.id,
        tool_type: yaml.tool_type,
        description: yaml.description,
        tags: yaml.tags,
        configuration,
    })
}

fn yaml_val_to_json(v: &serde_yaml::Value) -> serde_json::Value {
    match v {
        serde_yaml::Value::Null => serde_json::Value::Null,
        serde_yaml::Value::Bool(b) => serde_json::Value::Bool(*b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                serde_json::Value::Number(i.into())
            } else if let Some(f) = n.as_f64() {
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::Value::Null
            }
        }
        serde_yaml::Value::String(s) => serde_json::Value::String(s.clone()),
        serde_yaml::Value::Sequence(seq) => {
            serde_json::Value::Array(seq.iter().map(yaml_val_to_json).collect())
        }
        serde_yaml::Value::Mapping(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .filter_map(|(k, v)| k.as_str().map(|s| (s.to_string(), yaml_val_to_json(v))))
                .collect();
            serde_json::Value::Object(obj)
        }
        serde_yaml::Value::Tagged(t) => yaml_val_to_json(&t.value),
    }
}

fn build_esql_config(yaml: &ToolYaml) -> Result<serde_json::Value> {
    let query = yaml
        .extra
        .get("query")
        .and_then(|v| v.as_str())
        .context("esql tool requires a 'query' field")?
        .to_string();

    let mut config = serde_json::json!({ "query": query });

    if let Some(params_val) = yaml.extra.get("params") {
        let params_map: HashMap<String, EsqlParamYaml> = serde_yaml::from_value(params_val.clone())
            .context("failed to parse 'params' in esql tool")?;

        let mut json_params = serde_json::Map::new();
        for (name, param) in &params_map {
            let mut p = serde_json::Map::new();
            p.insert("type".to_string(), serde_json::Value::String(param.param_type.clone()));
            if !param.description.is_empty() {
                p.insert("description".to_string(), serde_json::Value::String(param.description.clone()));
            }
            if param.optional {
                p.insert("optional".to_string(), serde_json::Value::Bool(true));
            }
            if let Some(dv) = &param.default_value {
                p.insert("defaultValue".to_string(), yaml_val_to_json(dv));
            }
            json_params.insert(name.clone(), serde_json::Value::Object(p));
        }
        config["params"] = serde_json::Value::Object(json_params);
    }

    Ok(config)
}

fn build_index_search_config(yaml: &ToolYaml) -> Result<serde_json::Value> {
    let pattern = yaml
        .extra
        .get("pattern")
        .and_then(|v| v.as_str())
        .context("index_search tool requires a 'pattern' field")?
        .to_string();

    let mut config = serde_json::json!({ "pattern": pattern });

    if let Some(rl) = yaml.extra.get("row_limit") {
        config["row_limit"] = yaml_val_to_json(rl);
    }
    if let Some(ci) = yaml.extra.get("custom_instructions") {
        config["custom_instructions"] = yaml_val_to_json(ci);
    }

    Ok(config)
}

fn build_workflow_config(yaml: &ToolYaml) -> Result<serde_json::Value> {
    let workflow_id = yaml
        .extra
        .get("workflow_id")
        .and_then(|v| v.as_str())
        .context("workflow tool requires a 'workflow_id' field")?
        .to_string();

    let wait = yaml
        .extra
        .get("wait_for_completion")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok(serde_json::json!({
        "workflow_id": workflow_id,
        "wait_for_completion": wait,
    }))
}

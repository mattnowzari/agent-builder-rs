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
pub struct ToolStep {
    pub tool_id: String,
    pub reasoning: Option<String>,
    pub params_summary: String,
    pub result_summary: String,
}

#[derive(Debug, Clone)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
    pub steps: Vec<ToolStep>,
}

#[derive(Debug, Clone)]
pub struct ConversationDetail {
    pub messages: Vec<ConversationMessage>,
    /// LLM model name extracted from the most recent round's `model_usage`.
    pub model_name: Option<String>,
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
    pub enable_elastic_capabilities: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolSummary {
    pub id: String,
    #[serde(default)]
    #[allow(dead_code)]
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

const API_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

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
            .connect_timeout(std::time::Duration::from_secs(10))
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
            capabilities: ConverseCapabilities { visualizations: true },
            attachments: vec![ConverseAttachment {
                attachment_type: "screen_context",
                data: ScreenContextData {
                    url: format!(
                        "{}/app/management/insightsAndAlerting/triggersActionsConnectors",
                        self.base_url
                    ),
                    app: "management",
                },
                hidden: true,
            }],
        };

        let resp = self
            .http
            .post(url)
            .timeout(std::time::Duration::from_secs(300))
            .json(&body)
            .send()
            .await
            .context("failed to send converse request (agent may still be processing)")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(msg) = v.pointer("/response/message").and_then(|m| m.as_str()) {
                    return Ok(ConverseResult {
                        conversation_id: v.get("conversation_id").and_then(|c| c.as_str()).map(String::from),
                        message: msg.to_string(),
                        model_name: extract_model_name(&v),
                        steps: extract_steps(&v),
                    });
                }
            }
            anyhow::bail!("Agent Builder API error {status}: {text}");
        }

        let v: serde_json::Value =
            serde_json::from_str(&text).context("failed to parse Agent Builder response JSON")?;

        let message = v
            .pointer("/response/message")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();

        let steps = extract_steps(&v);

        Ok(ConverseResult {
            conversation_id: v.get("conversation_id").and_then(|c| c.as_str()).map(String::from),
            message,
            model_name: extract_model_name(&v),
            steps,
        })
    }

    pub async fn list_agents(&self) -> Result<Vec<AgentSummary>> {
        let url = self.api_url("agents");
        let resp = self
            .http
            .get(url)
            .timeout(API_TIMEOUT)
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
            .timeout(API_TIMEOUT)
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
        let sent_elastic_caps = req.configuration.enable_elastic_capabilities;

        let url = self.api_url("agents");
        let resp = self
            .http
            .post(url)
            .timeout(API_TIMEOUT)
            .json(&req)
            .send()
            .await
            .context("failed to send create agent request")?;

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
            enable_elastic_capabilities: sent_elastic_caps,
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
        let sent_elastic_caps = req.configuration.enable_elastic_capabilities;

        let url = self.api_url(&format!("agents/{id}"));
        let resp = self
            .http
            .put(url)
            .timeout(API_TIMEOUT)
            .json(&req)
            .send()
            .await
            .context("failed to send update agent request")?;

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
            enable_elastic_capabilities: sent_elastic_caps,
        })
    }

    pub async fn list_skills(&self) -> Result<Vec<SkillSummary>> {
        let url = format!("{}?include_plugins=true", self.api_url("skills"));
        let resp = self
            .http
            .get(url)
            .timeout(API_TIMEOUT)
            .send()
            .await
            .context("failed to send list skills request")?;

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
            .timeout(API_TIMEOUT)
            .send()
            .await
            .context("failed to send list plugins request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Agent Builder API error {status}: {text}");
        }

        let parsed: ListPluginsResponse =
            serde_json::from_str(&text).context("failed to parse list plugins response JSON")?;
        Ok(parsed.results)
    }

    pub async fn install_plugin(&self, plugin_url: &str) -> Result<PluginSummary> {
        let url = self.api_url("plugins/install");

        let body = serde_json::json!({ "url": plugin_url });

        let resp = self
            .http
            .post(&url)
            .timeout(API_TIMEOUT)
            .json(&body)
            .send()
            .await
            .context("failed to send plugin install request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Agent Builder API error {status}: {text}");
        }

        let plugin: PluginSummary =
            serde_json::from_str(&text).context("failed to parse plugin install response JSON")?;
        Ok(plugin)
    }

    pub async fn list_conversations(&self) -> Result<Vec<ConversationSummary>> {
        let url = self.api_url("conversations");
        let resp = self
            .http
            .get(url)
            .timeout(API_TIMEOUT)
            .send()
            .await
            .context("failed to send list conversations request")?;

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
            .timeout(API_TIMEOUT)
            .send()
            .await
            .context("failed to send get conversation request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Agent Builder API error {status}: {text}");
        }

        let v: serde_json::Value = serde_json::from_str(&text)
            .context("failed to parse get conversation response JSON")?;
        parse_conversation_detail(v).context("failed to parse conversation detail")
    }

    pub async fn create_tool(&self, req: &CreateToolRequest) -> Result<ToolSummary> {
        let url = self.api_url("tools");
        let resp = self
            .http
            .post(url)
            .timeout(API_TIMEOUT)
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

    pub async fn create_skill(&self, req: &CreateSkillRequest) -> Result<SkillSummary> {
        let url = self.api_url("skills");
        let resp = self
            .http
            .post(url)
            .timeout(API_TIMEOUT)
            .json(req)
            .send()
            .await
            .context("failed to send create skill request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Agent Builder API error {status}: {text}");
        }

        let skill: SkillSummary =
            serde_json::from_str(&text).context("failed to parse create skill response")?;
        Ok(skill)
    }

    pub async fn delete_agent(&self, id: &str) -> Result<()> {
        let url = self.api_url(&format!("agents/{id}"));
        let resp = self
            .http
            .delete(url)
            .timeout(API_TIMEOUT)
            .send()
            .await
            .context("failed to send delete agent request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Agent Builder API error {status}: {text}");
        }
        Ok(())
    }

    pub async fn delete_conversation(&self, id: &str) -> Result<()> {
        let url = self.api_url(&format!("conversations/{id}"));
        let resp = self
            .http
            .delete(url)
            .timeout(API_TIMEOUT)
            .send()
            .await
            .context("failed to send delete conversation request")?;

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

        let enable_elastic_capabilities = config
            .and_then(|v| v.get("enable_elastic_capabilities"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        out.push(AgentSummary {
            id,
            name,
            description,
            instructions,
            tool_ids,
            skill_ids,
            plugin_ids,
            enable_elastic_capabilities,
        });
    }

    out.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));
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

fn parse_conversation_detail(v: serde_json::Value) -> Result<ConversationDetail> {
    let obj = v.as_object().context("conversation detail is not an object")?;

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
                    messages.push(ConversationMessage { role, content, steps: Vec::new() });
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
                        steps: Vec::new(),
                    });
                }

                // Agent response + tool-call steps
                let response = round_obj
                    .get("response")
                    .and_then(|v| v.get("message"))
                    .and_then(|v| v.as_str())
                    .or_else(|| round_obj.get("output").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string();

                let steps = extract_steps(&serde_json::Value::Object(round_obj.clone()));

                if !response.is_empty() {
                    messages.push(ConversationMessage {
                        role: "assistant".to_string(),
                        content: response,
                        steps,
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
                    messages.push(ConversationMessage { role, content, steps: Vec::new() });
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
            steps: Vec::new(),
        });
    }

    let model_name = extract_model_from_rounds(obj);

    Ok(ConversationDetail { messages, model_name })
}

/// Extract `model_usage.model` from the sync converse response (top-level `model_usage`).
fn extract_model_name(v: &serde_json::Value) -> Option<String> {
    v.get("model_usage")
        .and_then(|mu| mu.get("model"))
        .and_then(|m| m.as_str())
        .map(String::from)
}

/// Extract tool-call and reasoning steps from a converse response or a single round.
/// The API returns `steps[]` at the top level (converse) or inside each round (history).
/// Each step has a `type` field: `"tool_call"` or `"reasoning"`.
fn extract_steps(v: &serde_json::Value) -> Vec<ToolStep> {
    let steps_arr = v
        .get("steps")
        .or_else(|| v.pointer("/response/steps"))
        .and_then(|s| s.as_array());

    let Some(arr) = steps_arr else {
        return Vec::new();
    };

    arr.iter()
        .filter_map(|step| {
            let step_type = step.get("type").and_then(|t| t.as_str()).unwrap_or("");

            match step_type {
                "tool_call" => {
                    let tool_id = step.get("tool_id").and_then(|t| t.as_str()).unwrap_or("unknown");
                    let params = step.get("params").cloned().unwrap_or(serde_json::Value::Null);

                    // Results is an array; summarise the first entry's data
                    let result = step
                        .get("results")
                        .and_then(|r| r.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|entry| entry.get("data"))
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);

                    Some(ToolStep {
                        tool_id: tool_id.to_string(),
                        reasoning: None,
                        params_summary: summarise_json(&params, 120),
                        result_summary: summarise_json(&result, 200),
                    })
                }
                "reasoning" => {
                    let text = step.get("reasoning").and_then(|r| r.as_str()).unwrap_or("");
                    if text.is_empty() {
                        return None;
                    }
                    Some(ToolStep {
                        tool_id: String::new(),
                        reasoning: Some(text.to_string()),
                        params_summary: String::new(),
                        result_summary: String::new(),
                    })
                }
                _ => None,
            }
        })
        .collect()
}

/// Produce a compact, truncated string representation of a JSON value.
fn summarise_json(v: &serde_json::Value, max_len: usize) -> String {
    let raw = match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => serde_json::to_string(other).unwrap_or_default(),
    };
    if raw.len() <= max_len {
        raw
    } else {
        format!("{}…", &raw[..max_len])
    }
}

/// Extract model name from the last round's `model_usage` in a conversation detail.
fn extract_model_from_rounds(obj: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    obj.get("rounds")
        .and_then(|v| v.as_array())
        .and_then(|rounds| rounds.last())
        .and_then(|round| round.get("model_usage"))
        .and_then(|mu| mu.get("model"))
        .and_then(|m| m.as_str())
        .map(String::from)
}

// --- Request / Response types ---

#[derive(Debug, Serialize)]
struct ConverseCapabilities {
    visualizations: bool,
}

#[derive(Debug, Serialize)]
struct ConverseAttachment {
    #[serde(rename = "type")]
    attachment_type: &'static str,
    data: ScreenContextData,
    hidden: bool,
}

#[derive(Debug, Serialize)]
struct ScreenContextData {
    url: String,
    app: &'static str,
}

#[derive(Debug, Serialize)]
struct ConverseRequest<'a> {
    input: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    agent_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    conversation_id: Option<&'a str>,
    capabilities: ConverseCapabilities,
    attachments: Vec<ConverseAttachment>,
}

#[derive(Debug, Clone)]
pub struct ConverseResult {
    pub conversation_id: Option<String>,
    pub message: String,
    pub model_name: Option<String>,
    pub steps: Vec<ToolStep>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
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
    pub enable_elastic_capabilities: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Esql,
    IndexSearch,
    Workflow,
}

impl std::fmt::Display for ToolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolKind::Esql => f.write_str("esql"),
            ToolKind::IndexSearch => f.write_str("index_search"),
            ToolKind::Workflow => f.write_str("workflow"),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ToolYaml {
    id: String,
    #[serde(rename = "type")]
    tool_type: ToolKind,
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
    pub tool_type: ToolKind,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub configuration: serde_json::Value,
}

pub fn parse_tool_yaml(contents: &str) -> Result<CreateToolRequest> {
    let yaml: ToolYaml =
        serde_yaml::from_str(contents).context("failed to parse YAML tool definition")?;

    let configuration = match yaml.tool_type {
        ToolKind::Esql => build_esql_config(&yaml)?,
        ToolKind::IndexSearch => build_index_search_config(&yaml)?,
        ToolKind::Workflow => build_workflow_config(&yaml)?,
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

// ---------------------------------------------------------------------------
// YAML skill loading
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SkillYaml {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Relative path to the main markdown file (resolved from the YAML file's parent dir).
    pub content: String,
    #[serde(default)]
    pub referenced_content: Vec<SkillRefYaml>,
    #[serde(default)]
    pub tool_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SkillRefYaml {
    pub name: String,
    /// Relative path to a supplementary markdown file.
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct CreateSkillRequest {
    pub id: String,
    pub name: String,
    pub description: String,
    pub content: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub referenced_content: Vec<SkillReferencedContent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillReferencedContent {
    pub name: String,
    pub relative_path: String,
    pub content: String,
}

pub fn parse_skill_yaml(contents: &str) -> Result<SkillYaml> {
    serde_yaml::from_str(contents).context("failed to parse YAML skill definition")
}

// ---------------------------------------------------------------------------
// YAML agent loading
// ---------------------------------------------------------------------------

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct AgentYaml {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Relative path to the markdown instructions file.
    pub instructions: String,
    #[serde(default)]
    pub tool_ids: Vec<String>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    #[serde(default)]
    pub plugin_ids: Vec<String>,
    #[serde(default = "default_true")]
    pub enable_elastic_capabilities: bool,
    pub avatar_color: Option<String>,
    pub avatar_symbol: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    pub visibility: Option<String>,
}

pub fn parse_agent_yaml(contents: &str) -> Result<AgentYaml> {
    serde_yaml::from_str(contents).context("failed to parse YAML agent definition")
}

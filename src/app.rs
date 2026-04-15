use anyhow::Result;
use ratatui::DefaultTerminal;

use crate::elm::{Model, Msg, update, view};

pub fn run() -> Result<()> {
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = ratatui::crossterm::execute!(
            std::io::stdout(),
            ratatui::crossterm::event::DisableMouseCapture
        );
        ratatui::restore();

        let bt = std::backtrace::Backtrace::capture();
        eprintln!("\n\nAgent Builder TUI panicked: {info}\n\nBacktrace:\n{bt}\n");
        prev_hook(info);
    }));

    let terminal = ratatui::init();
    ratatui::crossterm::execute!(
        std::io::stdout(),
        ratatui::crossterm::event::EnableMouseCapture
    )?;

    let result = run_with_terminal(terminal);

    let _ = ratatui::crossterm::execute!(
        std::io::stdout(),
        ratatui::crossterm::event::DisableMouseCapture
    );
    ratatui::restore();
    result
}

fn run_with_terminal(mut terminal: DefaultTerminal) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Msg>();

    let mut model = Model::default();
    let mut queue = std::collections::VecDeque::<Msg>::new();

    queue.push_back(Msg::Init);

    while !model.should_quit {
        while let Ok(msg) = rx.try_recv() {
            queue.push_back(msg);
        }

        if let Some(msg) = read_msg()? {
            queue.push_back(msg);
        }

        while let Some(msg) = queue.pop_front() {
            let cmds = update(&mut model, msg);
            for cmd in cmds {
                execute_cmd(&rt, tx.clone(), &model, cmd);
            }
        }

        terminal.draw(|frame| view(frame, &mut model))?;
    }

    Ok(())
}

fn read_msg() -> Result<Option<Msg>> {
    use ratatui::crossterm::event::{self, Event, KeyCode};
    use std::time::Duration;

    if !event::poll(Duration::from_millis(250))? {
        return Ok(Some(Msg::Tick));
    }

    match event::read()? {
        Event::Key(key) => {
            if key.code == KeyCode::Char('c')
                && key.modifiers.contains(event::KeyModifiers::CONTROL)
            {
                Ok(Some(Msg::Quit))
            } else {
                Ok(Some(Msg::Key(key)))
            }
        }
        Event::Mouse(mouse) => Ok(Some(Msg::Mouse(mouse))),
        Event::Resize(_, _) => Ok(Some(Msg::Resize)),
        _ => Ok(None),
    }
}

/// Spawn an API task: checks config readiness, builds the client, runs `work`,
/// and sends the appropriate success or failure message.
fn spawn_api<F, Fut>(
    rt: &tokio::runtime::Runtime,
    cfg: std::sync::Arc<crate::config::Config>,
    tx: tokio::sync::mpsc::UnboundedSender<Msg>,
    on_fail: impl FnOnce(String) -> Msg + Send + 'static,
    work: F,
)
where
    F: FnOnce(crate::agent_builder::AgentBuilderClient, tokio::sync::mpsc::UnboundedSender<Msg>) -> Fut
        + Send
        + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    rt.spawn(async move {
        if !cfg.is_ready() {
            let _ = tx.send(on_fail("Missing KIBANA_URL and/or API_KEY.".to_string()));
            return;
        }
        let client = match crate::agent_builder::AgentBuilderClient::new(&cfg) {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(on_fail(format!("{e:#}")));
                return;
            }
        };
        work(client, tx).await;
    });
}

fn execute_cmd(
    rt: &tokio::runtime::Runtime,
    tx: tokio::sync::mpsc::UnboundedSender<Msg>,
    model: &Model,
    cmd: crate::elm::Cmd,
) {
    use crate::elm::Cmd;

    match cmd {
        Cmd::LoadEnv => {
            rt.spawn(async move {
                let cfg = tokio::task::spawn_blocking(crate::config::load_from_env)
                    .await
                    .unwrap_or_default();
                let _ = tx.send(Msg::EnvLoaded { config: cfg });
            });
        }

        Cmd::LoadAgents => {
            let generation = model.agents_generation;
            spawn_api(rt, model.config.clone(), tx, move |e| Msg::AgentsLoadFailed { error: e, generation }, move |client, tx| async move {
                match client.list_agents().await {
                    Ok(agents) => { let _ = tx.send(Msg::AgentsLoaded { agents, generation }); }
                    Err(e) => { let _ = tx.send(Msg::AgentsLoadFailed { error: format!("{e:#}"), generation }); }
                }
            });
        }

        Cmd::LoadConversations => {
            spawn_api(rt, model.config.clone(), tx, |e| Msg::ConversationsLoadFailed { error: e }, |client, tx| async move {
                match client.list_conversations().await {
                    Ok(conversations) => { let _ = tx.send(Msg::ConversationsLoaded { conversations }); }
                    Err(e) => { let _ = tx.send(Msg::ConversationsLoadFailed { error: format!("{e:#}") }); }
                }
            });
        }

        Cmd::LoadConversationHistory { conversation_id } => {
            let cid = conversation_id.clone();
            let cid2 = conversation_id;
            spawn_api(rt, model.config.clone(), tx, move |e| Msg::ConversationHistoryFailed { conversation_id: cid, error: e }, move |client, tx| async move {
                match client.get_conversation(&cid2).await {
                    Ok(detail) => {
                        let messages: Vec<(String, String)> = detail.messages.into_iter().map(|m| (m.role, m.content)).collect();
                        let _ = tx.send(Msg::ConversationHistoryLoaded { conversation_id: cid2, messages, model_name: detail.model_name });
                    }
                    Err(e) => { let _ = tx.send(Msg::ConversationHistoryFailed { conversation_id: cid2, error: format!("{e:#}") }); }
                }
            });
        }

        Cmd::LoadTools => {
            spawn_api(rt, model.config.clone(), tx, |e| Msg::ToolsLoadFailed { error: e }, |client, tx| async move {
                match client.list_tools().await {
                    Ok(tools) => { let _ = tx.send(Msg::ToolsLoaded { tools }); }
                    Err(e) => { let _ = tx.send(Msg::ToolsLoadFailed { error: format!("{e:#}") }); }
                }
            });
        }

        Cmd::LoadSkills => {
            spawn_api(rt, model.config.clone(), tx, |e| Msg::SkillsLoadFailed { error: e }, |client, tx| async move {
                match client.list_skills().await {
                    Ok(skills) => { let _ = tx.send(Msg::SkillsLoaded { skills }); }
                    Err(e) => { let _ = tx.send(Msg::SkillsLoadFailed { error: format!("{e:#}") }); }
                }
            });
        }

        Cmd::LoadPlugins => {
            spawn_api(rt, model.config.clone(), tx, |e| Msg::PluginsLoadFailed { error: e }, |client, tx| async move {
                match client.list_plugins().await {
                    Ok(plugins) => { let _ = tx.send(Msg::PluginsLoaded { plugins }); }
                    Err(e) => { let _ = tx.send(Msg::PluginsLoadFailed { error: format!("{e:#}") }); }
                }
            });
        }

        Cmd::LoadComponentsData => {
            let generation = model.components_generation;
            spawn_api(rt, model.config.clone(), tx, move |e| Msg::ComponentsDataFailed { error: e, generation }, move |client, tx| async move {
                let mut errors = Vec::new();
                let tools = match client.list_tools().await {
                    Ok(t) => t,
                    Err(e) => { errors.push(format!("tools: {e}")); vec![] }
                };
                let skills = match client.list_skills().await {
                    Ok(s) => s,
                    Err(e) => { errors.push(format!("skills: {e}")); vec![] }
                };
                let plugins = match client.list_plugins().await {
                    Ok(p) => p,
                    Err(e) => { errors.push(format!("plugins: {e}")); vec![] }
                };

                if errors.is_empty() {
                    let _ = tx.send(Msg::ComponentsDataLoaded { tools, skills, plugins, generation });
                } else {
                    let _ = tx.send(Msg::ComponentsDataFailed { error: errors.join("; "), generation });
                }
            });
        }

        Cmd::SendPrompt { text } => {
            let conversation_id = model
                .active_session()
                .and_then(|s| s.conversation_id.clone());
            spawn_api(rt, model.config.clone(), tx, |e| Msg::PromptResponseFailed { error: e }, move |client, tx| async move {
                match client.converse(&text, conversation_id.as_deref()).await {
                    Ok(res) => { let _ = tx.send(Msg::PromptResponseReceived { content: res.message, conversation_id: res.conversation_id, model_name: res.model_name }); }
                    Err(e) => { let _ = tx.send(Msg::PromptResponseFailed { error: format!("{e:#}") }); }
                }
            });
        }

        Cmd::UpsertAgent {
            is_edit,
            id,
            name,
            description,
            instructions,
            tool_ids,
            skill_ids,
            plugin_ids,
            enable_elastic_capabilities,
        } => {
            spawn_api(rt, model.config.clone(), tx, move |e| Msg::AgentUpsertFailed { error: e }, move |client, tx| async move {
                let config = crate::agent_builder::AgentConfiguration {
                    instructions: Some(instructions),
                    tools: vec![crate::agent_builder::AgentTools { tool_ids }],
                    skill_ids,
                    plugin_ids,
                    enable_elastic_capabilities,
                };

                let res = if is_edit {
                    client.update_agent(&id, crate::agent_builder::UpdateAgentRequest {
                        name, description, configuration: config,
                        avatar_color: None, avatar_symbol: None, labels: vec![],
                    }).await
                } else {
                    client.create_agent(crate::agent_builder::CreateAgentRequest {
                        id, name, description, configuration: config,
                        avatar_color: None, avatar_symbol: None, labels: vec![],
                        visibility: None,
                    }).await
                };

                match res {
                    Ok(agent) => { let _ = tx.send(Msg::AgentUpserted { agent, is_edit }); }
                    Err(e) => { let _ = tx.send(Msg::AgentUpsertFailed { error: format!("{e:#}") }); }
                }
            });
        }

        Cmd::ImportComponentFromFile { path, component_type } => {
            let cfg = model.config.clone();
            rt.spawn(async move {
                use crate::elm::ComponentsTab;

                match component_type {
                    ComponentsTab::Tools => {
                        let contents = match tokio::fs::read_to_string(&path).await {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = tx.send(Msg::ToolCreateFromFileFailed { error: format!("Failed to read file: {e}") });
                                return;
                            }
                        };
                        let req = match crate::agent_builder::parse_tool_yaml(&contents) {
                            Ok(r) => r,
                            Err(e) => {
                                let _ = tx.send(Msg::ToolCreateFromFileFailed { error: format!("YAML parse error: {e}") });
                                return;
                            }
                        };
                        if !cfg.is_ready() {
                            let _ = tx.send(Msg::ToolCreateFromFileFailed { error: "Missing KIBANA_URL and/or API_KEY.".to_string() });
                            return;
                        }
                        let client = match crate::agent_builder::AgentBuilderClient::new(&cfg) {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = tx.send(Msg::ToolCreateFromFileFailed { error: format!("{e:#}") });
                                return;
                            }
                        };
                        match client.create_tool(&req).await {
                            Ok(tool) => { let _ = tx.send(Msg::ToolCreatedFromFile { tool }); }
                            Err(e) => { let _ = tx.send(Msg::ToolCreateFromFileFailed { error: format!("{e:#}") }); }
                        }
                    }
                    ComponentsTab::Skills => {
                        let yaml_path = std::path::PathBuf::from(&path);
                        let parent = match yaml_path.parent() {
                            Some(p) => p.to_path_buf(),
                            None => {
                                let _ = tx.send(Msg::SkillCreateFromFileFailed { error: "Cannot determine parent directory of YAML file.".to_string() });
                                return;
                            }
                        };

                        let contents = match tokio::fs::read_to_string(&path).await {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = tx.send(Msg::SkillCreateFromFileFailed { error: format!("Failed to read YAML file: {e}") });
                                return;
                            }
                        };
                        let skill_yaml = match crate::agent_builder::parse_skill_yaml(&contents) {
                            Ok(s) => s,
                            Err(e) => {
                                let _ = tx.send(Msg::SkillCreateFromFileFailed { error: format!("YAML parse error: {e:#}") });
                                return;
                            }
                        };

                        let content_md = match tokio::fs::read_to_string(parent.join(&skill_yaml.content)).await {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = tx.send(Msg::SkillCreateFromFileFailed {
                                    error: format!("Failed to read content file '{}': {e}", skill_yaml.content),
                                });
                                return;
                            }
                        };

                        let mut referenced = Vec::new();
                        for rc in &skill_yaml.referenced_content {
                            let rc_content = match tokio::fs::read_to_string(parent.join(&rc.path)).await {
                                Ok(c) => c,
                                Err(e) => {
                                    let _ = tx.send(Msg::SkillCreateFromFileFailed {
                                        error: format!("Failed to read referenced content '{}': {e}", rc.path),
                                    });
                                    return;
                                }
                            };
                            let filename = std::path::Path::new(&rc.path)
                                .file_name()
                                .and_then(|f| f.to_str())
                                .unwrap_or(&rc.path);
                            referenced.push(crate::agent_builder::SkillReferencedContent {
                                name: rc.name.clone(),
                                relative_path: format!("./{filename}"),
                                content: rc_content,
                            });
                        }

                        let req = crate::agent_builder::CreateSkillRequest {
                            id: skill_yaml.id,
                            name: skill_yaml.name,
                            description: skill_yaml.description,
                            content: content_md,
                            referenced_content: referenced,
                            tool_ids: skill_yaml.tool_ids,
                        };

                        if !cfg.is_ready() {
                            let _ = tx.send(Msg::SkillCreateFromFileFailed { error: "Missing KIBANA_URL and/or API_KEY.".to_string() });
                            return;
                        }
                        let client = match crate::agent_builder::AgentBuilderClient::new(&cfg) {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = tx.send(Msg::SkillCreateFromFileFailed { error: format!("{e:#}") });
                                return;
                            }
                        };
                        match client.create_skill(&req).await {
                            Ok(skill) => { let _ = tx.send(Msg::SkillCreatedFromFile { skill }); }
                            Err(e) => { let _ = tx.send(Msg::SkillCreateFromFileFailed { error: format!("{e:#}") }); }
                        }
                    }
                    ComponentsTab::Plugins => {
                        // Plugins are installed via URL, not from a local file.
                        // This branch should not be reached.
                    }
                }
            });
        }

        Cmd::InstallPluginFromUrl { url } => {
            let download_url = crate::github::github_url_to_download_zip(&url)
                .unwrap_or(url);
            spawn_api(rt, model.config.clone(), tx, |e| Msg::PluginInstallFromFileFailed { error: e }, move |client, tx| async move {
                match client.install_plugin(&download_url).await {
                    Ok(plugin) => { let _ = tx.send(Msg::PluginInstalledFromFile { plugin }); }
                    Err(e) => { let _ = tx.send(Msg::PluginInstallFromFileFailed { error: format!("{e:#}") }); }
                }
            });
        }

        Cmd::ImportComponentFromGitHub { url, component_type } => {
            let cfg = model.config.clone();
            rt.spawn(async move {
                use crate::elm::ComponentsTab;
                use crate::github::{derive_skill_yaml_path, parse_github_url};

                let (gh_ref, is_dir) = match parse_github_url(&url) {
                    Ok(v) => v,
                    Err(e) => {
                        let msg = match component_type {
                            ComponentsTab::Tools => Msg::ToolCreateFromFileFailed { error: format!("{e:#}") },
                            _ => Msg::SkillCreateFromFileFailed { error: format!("{e:#}") },
                        };
                        let _ = tx.send(msg);
                        return;
                    }
                };

                let http = reqwest::Client::builder()
                    .build()
                    .expect("failed to build HTTP client");

                let fetch_raw = |raw_url: String| {
                    let http = http.clone();
                    async move {
                        let resp = http.get(&raw_url).send().await
                            .map_err(|e| anyhow::anyhow!("fetch failed for {raw_url}: {e}"))?;
                        let status = resp.status();
                        if !status.is_success() {
                            anyhow::bail!("GitHub returned {status} for {raw_url}");
                        }
                        resp.text().await
                            .map_err(|e| anyhow::anyhow!("failed to read body from {raw_url}: {e}"))
                    }
                };

                match component_type {
                    ComponentsTab::Tools => {
                        if is_dir {
                            let _ = tx.send(Msg::ToolCreateFromFileFailed {
                                error: "Tool import expects a file URL (/blob/...), not a folder URL (/tree/...).".to_string(),
                            });
                            return;
                        }
                        let yaml_contents = match fetch_raw(gh_ref.raw_url_self()).await {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = tx.send(Msg::ToolCreateFromFileFailed { error: format!("{e:#}") });
                                return;
                            }
                        };
                        let req = match crate::agent_builder::parse_tool_yaml(&yaml_contents) {
                            Ok(r) => r,
                            Err(e) => {
                                let _ = tx.send(Msg::ToolCreateFromFileFailed { error: format!("YAML parse error: {e}") });
                                return;
                            }
                        };
                        if !cfg.is_ready() {
                            let _ = tx.send(Msg::ToolCreateFromFileFailed { error: "Missing KIBANA_URL and/or API_KEY.".to_string() });
                            return;
                        }
                        let client = match crate::agent_builder::AgentBuilderClient::new(&cfg) {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = tx.send(Msg::ToolCreateFromFileFailed { error: format!("{e:#}") });
                                return;
                            }
                        };
                        match client.create_tool(&req).await {
                            Ok(tool) => { let _ = tx.send(Msg::ToolCreatedFromFile { tool }); }
                            Err(e) => { let _ = tx.send(Msg::ToolCreateFromFileFailed { error: format!("{e:#}") }); }
                        }
                    }

                    ComponentsTab::Skills => {
                        let yaml_ref = if is_dir {
                            let yaml_path = derive_skill_yaml_path(&gh_ref.path);
                            crate::github::GitHubFileRef {
                                owner: gh_ref.owner.clone(),
                                repo: gh_ref.repo.clone(),
                                git_ref: gh_ref.git_ref.clone(),
                                path: yaml_path,
                            }
                        } else {
                            gh_ref
                        };

                        let yaml_contents = match fetch_raw(yaml_ref.raw_url_self()).await {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = tx.send(Msg::SkillCreateFromFileFailed { error: format!("{e:#}") });
                                return;
                            }
                        };
                        let skill_yaml = match crate::agent_builder::parse_skill_yaml(&yaml_contents) {
                            Ok(s) => s,
                            Err(e) => {
                                let _ = tx.send(Msg::SkillCreateFromFileFailed { error: format!("YAML parse error: {e:#}") });
                                return;
                            }
                        };

                        let content_path = yaml_ref.resolve_relative(&skill_yaml.content);
                        let content_md = match fetch_raw(yaml_ref.raw_url(&content_path)).await {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = tx.send(Msg::SkillCreateFromFileFailed {
                                    error: format!("Failed to fetch content file '{}': {e:#}", skill_yaml.content),
                                });
                                return;
                            }
                        };

                        let mut referenced = Vec::new();
                        for rc in &skill_yaml.referenced_content {
                            let rc_path = yaml_ref.resolve_relative(&rc.path);
                            let rc_content = match fetch_raw(yaml_ref.raw_url(&rc_path)).await {
                                Ok(c) => c,
                                Err(e) => {
                                    let _ = tx.send(Msg::SkillCreateFromFileFailed {
                                        error: format!("Failed to fetch referenced content '{}': {e:#}", rc.path),
                                    });
                                    return;
                                }
                            };
                            let filename = std::path::Path::new(&rc.path)
                                .file_name()
                                .and_then(|f| f.to_str())
                                .unwrap_or(&rc.path);
                            referenced.push(crate::agent_builder::SkillReferencedContent {
                                name: rc.name.clone(),
                                relative_path: format!("./{filename}"),
                                content: rc_content,
                            });
                        }

                        let req = crate::agent_builder::CreateSkillRequest {
                            id: skill_yaml.id,
                            name: skill_yaml.name,
                            description: skill_yaml.description,
                            content: content_md,
                            referenced_content: referenced,
                            tool_ids: skill_yaml.tool_ids,
                        };

                        if !cfg.is_ready() {
                            let _ = tx.send(Msg::SkillCreateFromFileFailed { error: "Missing KIBANA_URL and/or API_KEY.".to_string() });
                            return;
                        }
                        let client = match crate::agent_builder::AgentBuilderClient::new(&cfg) {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = tx.send(Msg::SkillCreateFromFileFailed { error: format!("{e:#}") });
                                return;
                            }
                        };
                        match client.create_skill(&req).await {
                            Ok(skill) => { let _ = tx.send(Msg::SkillCreatedFromFile { skill }); }
                            Err(e) => { let _ = tx.send(Msg::SkillCreateFromFileFailed { error: format!("{e:#}") }); }
                        }
                    }

                    ComponentsTab::Plugins => {
                        // Plugins use the InstallPluginFromUrl command instead.
                    }
                }
            });
        }

        Cmd::ImportAgentFromFile { path } => {
            let cfg = model.config.clone();
            rt.spawn(async move {
                let yaml_path = std::path::PathBuf::from(&path);
                let parent = match yaml_path.parent() {
                    Some(p) => p.to_path_buf(),
                    None => {
                        let _ = tx.send(Msg::AgentUpsertFailed {
                            error: "Cannot determine parent directory of YAML file.".to_string(),
                        });
                        return;
                    }
                };

                let contents = match tokio::fs::read_to_string(&path).await {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Msg::AgentUpsertFailed {
                            error: format!("Failed to read YAML file: {e}"),
                        });
                        return;
                    }
                };
                let agent_yaml = match crate::agent_builder::parse_agent_yaml(&contents) {
                    Ok(a) => a,
                    Err(e) => {
                        let _ = tx.send(Msg::AgentUpsertFailed {
                            error: format!("YAML parse error: {e:#}"),
                        });
                        return;
                    }
                };

                let instructions = match tokio::fs::read_to_string(parent.join(&agent_yaml.instructions)).await {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Msg::AgentUpsertFailed {
                            error: format!("Failed to read instructions file '{}': {e}", agent_yaml.instructions),
                        });
                        return;
                    }
                };

                if !cfg.is_ready() {
                    let _ = tx.send(Msg::AgentUpsertFailed {
                        error: "Missing KIBANA_URL and/or API_KEY.".to_string(),
                    });
                    return;
                }
                let client = match crate::agent_builder::AgentBuilderClient::new(&cfg) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Msg::AgentUpsertFailed { error: format!("{e:#}") });
                        return;
                    }
                };

                let req = crate::agent_builder::CreateAgentRequest {
                    id: agent_yaml.id,
                    name: agent_yaml.name,
                    description: agent_yaml.description,
                    configuration: crate::agent_builder::AgentConfiguration {
                        instructions: Some(instructions),
                        tools: vec![crate::agent_builder::AgentTools { tool_ids: agent_yaml.tool_ids }],
                        skill_ids: agent_yaml.skill_ids,
                        plugin_ids: agent_yaml.plugin_ids,
                        enable_elastic_capabilities: agent_yaml.enable_elastic_capabilities,
                    },
                    avatar_color: agent_yaml.avatar_color,
                    avatar_symbol: agent_yaml.avatar_symbol,
                    labels: agent_yaml.labels,
                    visibility: agent_yaml.visibility,
                };

                match client.create_agent(req).await {
                    Ok(agent) => { let _ = tx.send(Msg::AgentUpserted { agent, is_edit: false }); }
                    Err(e) => { let _ = tx.send(Msg::AgentUpsertFailed { error: format!("{e:#}") }); }
                }
            });
        }

        Cmd::ImportAgentFromGitHub { url } => {
            let cfg = model.config.clone();
            rt.spawn(async move {
                use crate::github::{derive_skill_yaml_path, parse_github_url};

                let (gh_ref, is_dir) = match parse_github_url(&url) {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = tx.send(Msg::AgentUpsertFailed { error: format!("{e:#}") });
                        return;
                    }
                };

                let yaml_ref = if is_dir {
                    let yaml_path = derive_skill_yaml_path(&gh_ref.path);
                    crate::github::GitHubFileRef {
                        owner: gh_ref.owner.clone(),
                        repo: gh_ref.repo.clone(),
                        git_ref: gh_ref.git_ref.clone(),
                        path: yaml_path,
                    }
                } else {
                    gh_ref
                };

                let http = reqwest::Client::builder()
                    .build()
                    .expect("failed to build HTTP client");

                let fetch_raw = |raw_url: String| {
                    let http = http.clone();
                    async move {
                        let resp = http.get(&raw_url).send().await
                            .map_err(|e| anyhow::anyhow!("fetch failed for {raw_url}: {e}"))?;
                        let status = resp.status();
                        if !status.is_success() {
                            anyhow::bail!("GitHub returned {status} for {raw_url}");
                        }
                        resp.text().await
                            .map_err(|e| anyhow::anyhow!("failed to read body from {raw_url}: {e}"))
                    }
                };

                let yaml_contents = match fetch_raw(yaml_ref.raw_url_self()).await {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Msg::AgentUpsertFailed { error: format!("{e:#}") });
                        return;
                    }
                };
                let agent_yaml = match crate::agent_builder::parse_agent_yaml(&yaml_contents) {
                    Ok(a) => a,
                    Err(e) => {
                        let _ = tx.send(Msg::AgentUpsertFailed { error: format!("YAML parse error: {e:#}") });
                        return;
                    }
                };

                let instructions_path = yaml_ref.resolve_relative(&agent_yaml.instructions);
                let instructions = match fetch_raw(yaml_ref.raw_url(&instructions_path)).await {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Msg::AgentUpsertFailed {
                            error: format!("Failed to fetch instructions '{}': {e:#}", agent_yaml.instructions),
                        });
                        return;
                    }
                };

                if !cfg.is_ready() {
                    let _ = tx.send(Msg::AgentUpsertFailed {
                        error: "Missing KIBANA_URL and/or API_KEY.".to_string(),
                    });
                    return;
                }
                let client = match crate::agent_builder::AgentBuilderClient::new(&cfg) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Msg::AgentUpsertFailed { error: format!("{e:#}") });
                        return;
                    }
                };

                let req = crate::agent_builder::CreateAgentRequest {
                    id: agent_yaml.id,
                    name: agent_yaml.name,
                    description: agent_yaml.description,
                    configuration: crate::agent_builder::AgentConfiguration {
                        instructions: Some(instructions),
                        tools: vec![crate::agent_builder::AgentTools { tool_ids: agent_yaml.tool_ids }],
                        skill_ids: agent_yaml.skill_ids,
                        plugin_ids: agent_yaml.plugin_ids,
                        enable_elastic_capabilities: agent_yaml.enable_elastic_capabilities,
                    },
                    avatar_color: agent_yaml.avatar_color,
                    avatar_symbol: agent_yaml.avatar_symbol,
                    labels: agent_yaml.labels,
                    visibility: agent_yaml.visibility,
                };

                match client.create_agent(req).await {
                    Ok(agent) => { let _ = tx.send(Msg::AgentUpserted { agent, is_edit: false }); }
                    Err(e) => { let _ = tx.send(Msg::AgentUpsertFailed { error: format!("{e:#}") }); }
                }
            });
        }

        Cmd::DeleteAgent { id } => {
            let agent_name = model
                .agents
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.name.clone())
                .unwrap_or_else(|| id.clone());
            spawn_api(rt, model.config.clone(), tx, |e| Msg::AgentDeleteFailed { error: e }, move |client, tx| async move {
                if let Err(e) = client.delete_agent(&id).await {
                    let _ = tx.send(Msg::AgentDeleteFailed { error: format!("{e:#}") });
                    return;
                }
                let _ = tx.send(Msg::AgentDeleted { name: agent_name });
            });
        }
    }
}

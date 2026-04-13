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
            let cfg = model.config.clone();
            rt.spawn(async move {
                if !cfg.is_ready() {
                    let _ = tx.send(Msg::AgentsLoadFailed {
                        error: "Missing KIBANA_URL and/or API_KEY.".to_string(),
                    });
                    return;
                }
                let client = match crate::agentbuilder::AgentBuilderClient::new(&cfg) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Msg::AgentsLoadFailed {
                            error: e.to_string(),
                        });
                        return;
                    }
                };
                match client.list_agents().await {
                    Ok(agents) => {
                        let _ = tx.send(Msg::AgentsLoaded { agents });
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::AgentsLoadFailed {
                            error: e.to_string(),
                        });
                    }
                }
            });
        }

        Cmd::LoadConversations => {
            let cfg = model.config.clone();
            rt.spawn(async move {
                if !cfg.is_ready() {
                    let _ = tx.send(Msg::ConversationsLoadFailed {
                        error: "Missing KIBANA_URL and/or API_KEY.".to_string(),
                    });
                    return;
                }
                let client = match crate::agentbuilder::AgentBuilderClient::new(&cfg) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Msg::ConversationsLoadFailed {
                            error: e.to_string(),
                        });
                        return;
                    }
                };
                match client.list_conversations().await {
                    Ok(conversations) => {
                        let _ = tx.send(Msg::ConversationsLoaded { conversations });
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::ConversationsLoadFailed {
                            error: e.to_string(),
                        });
                    }
                }
            });
        }

        Cmd::LoadConversationHistory { conversation_id } => {
            let cfg = model.config.clone();
            let cid = conversation_id.clone();
            rt.spawn(async move {
                if !cfg.is_ready() {
                    let _ = tx.send(Msg::ConversationHistoryFailed {
                        conversation_id: cid,
                        error: "Missing KIBANA_URL and/or API_KEY.".to_string(),
                    });
                    return;
                }
                let client = match crate::agentbuilder::AgentBuilderClient::new(&cfg) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Msg::ConversationHistoryFailed {
                            conversation_id: cid,
                            error: e.to_string(),
                        });
                        return;
                    }
                };
                match client.get_conversation(&cid).await {
                    Ok(detail) => {
                        let messages: Vec<(String, String)> = detail
                            .messages
                            .into_iter()
                            .map(|m| (m.role, m.content))
                            .collect();
                        let _ = tx.send(Msg::ConversationHistoryLoaded {
                            conversation_id: cid,
                            messages,
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::ConversationHistoryFailed {
                            conversation_id: cid,
                            error: e.to_string(),
                        });
                    }
                }
            });
        }

        Cmd::LoadTools => {
            let cfg = model.config.clone();
            rt.spawn(async move {
                if !cfg.is_ready() {
                    let _ = tx.send(Msg::ToolsLoadFailed {
                        error: "Missing KIBANA_URL and/or API_KEY.".to_string(),
                    });
                    return;
                }
                let client = match crate::agentbuilder::AgentBuilderClient::new(&cfg) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Msg::ToolsLoadFailed {
                            error: e.to_string(),
                        });
                        return;
                    }
                };
                match client.list_tools().await {
                    Ok(tools) => {
                        let _ = tx.send(Msg::ToolsLoaded { tools });
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::ToolsLoadFailed {
                            error: e.to_string(),
                        });
                    }
                }
            });
        }

        Cmd::LoadSkills => {
            let cfg = model.config.clone();
            rt.spawn(async move {
                if !cfg.is_ready() {
                    let _ = tx.send(Msg::SkillsLoadFailed {
                        error: "Missing KIBANA_URL and/or API_KEY.".to_string(),
                    });
                    return;
                }
                let client = match crate::agentbuilder::AgentBuilderClient::new(&cfg) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Msg::SkillsLoadFailed {
                            error: e.to_string(),
                        });
                        return;
                    }
                };
                match client.list_skills().await {
                    Ok(skills) => {
                        let _ = tx.send(Msg::SkillsLoaded { skills });
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::SkillsLoadFailed {
                            error: e.to_string(),
                        });
                    }
                }
            });
        }

        Cmd::LoadPlugins => {
            let cfg = model.config.clone();
            rt.spawn(async move {
                if !cfg.is_ready() {
                    let _ = tx.send(Msg::PluginsLoadFailed {
                        error: "Missing KIBANA_URL and/or API_KEY.".to_string(),
                    });
                    return;
                }
                let client = match crate::agentbuilder::AgentBuilderClient::new(&cfg) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Msg::PluginsLoadFailed {
                            error: e.to_string(),
                        });
                        return;
                    }
                };
                match client.list_plugins().await {
                    Ok(plugins) => {
                        let _ = tx.send(Msg::PluginsLoaded { plugins });
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::PluginsLoadFailed {
                            error: e.to_string(),
                        });
                    }
                }
            });
        }

        Cmd::LoadComponentsData => {
            let cfg = model.config.clone();
            rt.spawn(async move {
                if !cfg.is_ready() {
                    let _ = tx.send(Msg::ComponentsDataFailed {
                        error: "Missing KIBANA_URL and/or API_KEY.".to_string(),
                    });
                    return;
                }
                let client = match crate::agentbuilder::AgentBuilderClient::new(&cfg) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Msg::ComponentsDataFailed {
                            error: e.to_string(),
                        });
                        return;
                    }
                };

                let tools = client.list_tools().await.unwrap_or_default();
                let skills = client.list_skills().await.unwrap_or_default();
                let plugins = client.list_plugins().await.unwrap_or_default();

                let _ = tx.send(Msg::ComponentsDataLoaded {
                    tools,
                    skills,
                    plugins,
                });
            });
        }

        Cmd::SendPrompt { text } => {
            let cfg = model.config.clone();
            let conversation_id = model
                .active_session()
                .and_then(|s| s.conversation_id.clone());
            rt.spawn(async move {
                if !cfg.is_ready() {
                    let _ = tx.send(Msg::PromptResponseFailed {
                        error: "Missing KIBANA_URL and/or API_KEY.".to_string(),
                    });
                    return;
                }
                let client = match crate::agentbuilder::AgentBuilderClient::new(&cfg) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Msg::PromptResponseFailed {
                            error: e.to_string(),
                        });
                        return;
                    }
                };
                match client.converse(&text, conversation_id.as_deref()).await {
                    Ok(res) => {
                        let _ = tx.send(Msg::PromptResponseReceived {
                            content: res.message,
                            conversation_id: res.conversation_id,
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::PromptResponseFailed {
                            error: e.to_string(),
                        });
                    }
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
        } => {
            let cfg = model.config.clone();
            rt.spawn(async move {
                if !cfg.is_ready() {
                    let _ = tx.send(Msg::AgentUpsertFailed {
                        error: "Missing KIBANA_URL and/or API_KEY.".to_string(),
                        is_edit,
                    });
                    return;
                }
                let client = match crate::agentbuilder::AgentBuilderClient::new(&cfg) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Msg::AgentUpsertFailed {
                            error: e.to_string(),
                            is_edit,
                        });
                        return;
                    }
                };

                let config = crate::agentbuilder::AgentConfiguration {
                    instructions: Some(instructions),
                    tools: vec![crate::agentbuilder::AgentTools { tool_ids }],
                    skill_ids,
                    plugin_ids,
                };

                let res = if is_edit {
                    client
                        .update_agent(
                            &id,
                            crate::agentbuilder::UpdateAgentRequest {
                                name,
                                description,
                                configuration: config,
                                avatar_color: None,
                                avatar_symbol: None,
                                labels: vec![],
                            },
                        )
                        .await
                } else {
                    client
                        .create_agent(crate::agentbuilder::CreateAgentRequest {
                            id,
                            name,
                            description,
                            configuration: config,
                            avatar_color: None,
                            avatar_symbol: None,
                            labels: vec![],
                        })
                        .await
                };

                match res {
                    Ok(agent) => {
                        let _ = tx.send(Msg::AgentUpserted { agent, is_edit });
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::AgentUpsertFailed {
                            error: e.to_string(),
                            is_edit,
                        });
                    }
                }
            });
        }

        Cmd::ImportComponentFromFile {
            path,
            component_type,
        } => {
            let cfg = model.config.clone();
            rt.spawn(async move {
                use crate::elm::ComponentsTab;

                match component_type {
                    ComponentsTab::Tools => {
                        let contents = match tokio::fs::read_to_string(&path).await {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = tx.send(Msg::ToolCreateFromFileFailed {
                                    error: format!("Failed to read file: {e}"),
                                });
                                return;
                            }
                        };

                        let req = match crate::agentbuilder::parse_tool_yaml(&contents) {
                            Ok(r) => r,
                            Err(e) => {
                                let _ = tx.send(Msg::ToolCreateFromFileFailed {
                                    error: format!("YAML parse error: {e}"),
                                });
                                return;
                            }
                        };

                        if !cfg.is_ready() {
                            let _ = tx.send(Msg::ToolCreateFromFileFailed {
                                error: "Missing KIBANA_URL and/or API_KEY.".to_string(),
                            });
                            return;
                        }
                        let client = match crate::agentbuilder::AgentBuilderClient::new(&cfg) {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = tx.send(Msg::ToolCreateFromFileFailed {
                                    error: e.to_string(),
                                });
                                return;
                            }
                        };

                        match client.create_tool(&req).await {
                            Ok(tool) => {
                                let _ = tx.send(Msg::ToolCreatedFromFile { tool });
                            }
                            Err(e) => {
                                let _ = tx.send(Msg::ToolCreateFromFileFailed {
                                    error: e.to_string(),
                                });
                            }
                        }
                    }
                    ComponentsTab::Skills => {
                        let _ = tx.send(Msg::ToolCreateFromFileFailed {
                            error: "Skill import from YAML is not yet implemented.".to_string(),
                        });
                    }
                    ComponentsTab::Plugins => {
                        let _ = tx.send(Msg::ToolCreateFromFileFailed {
                            error: "Plugin import from YAML is not yet implemented.".to_string(),
                        });
                    }
                }
            });
        }

        Cmd::DeleteAgent { id } => {
            let cfg = model.config.clone();
            let agent_name = model
                .agents
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.name.clone())
                .unwrap_or_else(|| id.clone());
            rt.spawn(async move {
                if !cfg.is_ready() {
                    let _ = tx.send(Msg::AgentDeleteFailed {
                        error: "Missing KIBANA_URL and/or API_KEY.".to_string(),
                    });
                    return;
                }
                let client = match crate::agentbuilder::AgentBuilderClient::new(&cfg) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Msg::AgentDeleteFailed {
                            error: e.to_string(),
                        });
                        return;
                    }
                };
                if let Err(e) = client.delete_agent(&id).await {
                    let _ = tx.send(Msg::AgentDeleteFailed {
                        error: e.to_string(),
                    });
                    return;
                }
                let _ = tx.send(Msg::AgentDeleted {
                    id,
                    name: agent_name,
                });
            });
        }
    }
}

# agent-builder-rs

A TUI (terminal user interface) version of [Kibana Agent Builder](https://www.elastic.co/docs/solutions/security/ai/ai-assistant-agent-builder). Create and manage agents, chat with them, browse conversation history, inspect tools/skills/plugins, and import custom components from YAML — all from your terminal.

> **Note:** This is a community project and is **not** an official Elastic product.

Built with [Ratatui](https://ratatui.rs/) and the Elm architecture (Model-Update-View + command/effect runtime).

![Rust](https://img.shields.io/badge/Rust-2024_edition-orange)

## Layout

The interface is split into four panels:

```
┌─────────────┬──────────────────────────┬─────────────┐
│   Agents    │                          │ Components  │
│   (list)    │         Chat             │  Plugins    │
├─────────────┤   (active conversation)  │  Skills     │
│   Chats     │                          │  Tools      │
│   (list)    │                          │             │
└─────────────┴──────────────────────────┴─────────────┘
```

| Panel | What it shows |
|-------|---------------|
| **Agents** (top-left) | All agents from the Agent Builder API. Select one to filter chats. |
| **Chats** (bottom-left) | Conversations for the selected agent. Pick one to load its history. |
| **Chat** (center) | The active conversation. Type a message and hit Enter to converse. |
| **Components** (right) | Three tabs — Plugins, Skills, Tools — showing available components. |

## Keybindings

| Key | Context | Action |
|-----|---------|--------|
| `Tab` / `Shift+Tab` | Global | Cycle panel focus |
| `j` / `k` or arrow keys | Any list | Navigate lists |
| `Enter` | Any list / Chat | Select item / send chat message |
| `n` | Agents panel | New chat session |
| `e` | Agents panel | Edit selected agent |
| `d` | Agents panel | Delete selected agent |
| `i` | Agents panel | Import agent from YAML |
| `g` | Agents panel | Import agent from GitHub URL |
| `i` | Components panel | Import tool/skill from YAML |
| `g` | Components panel | Import tool/skill from GitHub URL |
| `Left` / `Right` | Components panel | Switch Components tab |
| `Ctrl+R` | Any panel | Refresh current panel |
| `Ctrl+C` | Global | Quit |

## Getting Started

### Prerequisites

- **Rust** (edition 2024) — install via [rustup](https://rustup.rs/)
- A running **Kibana** instance with the **Agent Builder** feature enabled
- A Kibana **API key** with permissions to use the Agent Builder API

### Configuration

Create a `.env` file in the project root (or export the variables in your shell):

```env
KIBANA_URL="https://your-kibana-host:5601"
API_KEY="your-base64-encoded-api-key"
```

#### All environment variables

| Variable | Aliases | Required | Description |
|----------|---------|----------|-------------|
| `KIBANA_URL` | `ES_HOST`, `ELASTICSEARCH_HOST`, `ELASTIC_HOST` | Yes | Kibana base URL |
| `API_KEY` | `ES_API_KEY` | Yes | Base64-encoded Kibana API key |
| `KIBANA_SPACE` | `SPACE` | No | Kibana space prefix (e.g. `security`) |
| `AGENT_ID` | — | No | Default agent ID (defaults to `elastic-ai-agent`) |
| `KIBANA_INSECURE_TLS` | `INSECURE_TLS` | No | Accept self-signed certs (`1`, `true`, `yes`) |

### Build and run

The project includes a Makefile for common tasks:

```bash
make build            # compile (debug)
make build-release    # clean + compile (release, optimized)
make check            # type-check without building
make clippy           # run Clippy lints
make lint             # check formatting (rustfmt)
make autoformat       # auto-fix formatting
make test             # run tests
make clean            # cargo clean
make clean-build      # clean + build
```

To build and launch directly:

```bash
cargo run
```

If `KIBANA_URL` or `API_KEY` are missing, the TUI will show a modal telling you which variables need to be set.

## Importing Components

Tools, skills, and agents can be imported from local YAML files or directly from GitHub. Press `i` for local file import or `g` for GitHub import.

**Tools and Skills** — use `i` / `g` in the **Components** panel (switch to the Tools or Skills tab first).

**Agents** — use `i` / `g` in the **Agents** panel. An agent YAML folder contains a definition file plus a markdown instructions file (same pattern as skills).

```yaml
# agents/my-agent/my-agent.yaml
id: my-agent
name: My Agent
description: "What this agent does"
instructions: ./my-agent.md
tool_ids:
  - esql-user-lookup
skill_ids:
  - incident-response
enable_elastic_capabilities: true
```

### Tool YAML

A tool is a single YAML file. Three types are supported: `esql`, `index_search`, and `workflow`.

```yaml
id: esql-user-lookup
type: esql
description: "Look up users by email domain"
tags:
  - users
  - security
query: |
  FROM logs-system.auth-*
  | WHERE user.email LIKE ?domain
  | STATS login_count = COUNT(*) BY user.name
params:
  domain:
    type: string
    description: "Email domain pattern"
```

### Skill YAML

A skill is a folder containing a YAML definition and one or more markdown files:

```
skills/
  my-skill/
    my-skill.yaml        # select this file when importing
    my-skill.md           # main instructions (markdown)
    extra-context.md      # optional referenced content
```

```yaml
id: my-skill
name: My Skill
description: "What this skill does"
content: ./my-skill.md
referenced_content:
  - name: extra-context
    path: ./extra-context.md
tool_ids:
  - esql-user-lookup
```

See [docs/importing-components.md](docs/importing-components.md) for the full schema reference and more examples.

## Project Structure

```
src/
├── main.rs              # Entry point — calls app::run()
├── lib.rs               # Crate root — declares modules
├── app.rs               # Terminal setup, Tokio runtime, main loop, side-effect execution
├── config.rs            # Env-based configuration (.env / shell variables)
├── agent_builder.rs     # HTTP client for the Kibana Agent Builder REST API
├── github.rs            # GitHub URL parser and raw content URL builder
└── elm/
    ├── mod.rs           # Elm module root
    ├── model.rs         # All application state (Model)
    ├── msg.rs           # Event types (Msg)
    ├── cmd.rs           # Side-effect declarations (Cmd)
    ├── update.rs        # Pure state transitions: (Model, Msg) → Vec<Cmd>
    └── view.rs          # Rendering: draws the UI from Model
docs/
└── importing-components.md
tools/                   # Example tool YAML files
skills/                  # Example skill folders (YAML + markdown)
```

## Architecture

The app follows the **Elm architecture** (TEA):

- **Model** — a single struct holding all UI and session state.
- **Msg** — an enum of everything that can happen (key press, mouse event, tick, async API result).
- **update(model, msg) -> Vec\<Cmd\>** — a pure function that computes the next state and declares side effects as `Cmd` values. No I/O happens inside `update`.
- **view(frame, model)** — renders the current state to the terminal.
- **app.rs** bridges the gap: it runs a Tokio runtime that executes `Cmd` values as async tasks (HTTP requests, file reads) and sends their results back as `Msg` through an MPSC channel.

This keeps the core logic testable and deterministic — all async work is pushed to the edges.

## Dependencies

| Crate | Purpose |
|-------|---------|
| [ratatui](https://crates.io/crates/ratatui) | Terminal UI framework (layout, widgets, styling) |
| [ratatui-explorer](https://crates.io/crates/ratatui-explorer) | File picker widget for YAML import |
| [tokio](https://crates.io/crates/tokio) | Async runtime for API calls and file I/O |
| [reqwest](https://crates.io/crates/reqwest) | HTTP client for the Kibana Agent Builder API |
| [serde](https://crates.io/crates/serde) / [serde_json](https://crates.io/crates/serde_json) | JSON serialization/deserialization |
| [serde_yaml](https://crates.io/crates/serde_yaml) | YAML parsing for tool/skill imports |
| [dotenvy](https://crates.io/crates/dotenvy) | Load `.env` files |
| [anyhow](https://crates.io/crates/anyhow) | Ergonomic error handling |

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.

## License

This project is licensed under the [GNU Affero General Public License v3.0](https://www.gnu.org/licenses/agpl-3.0.html) (AGPL-3.0). See [LICENSE](LICENSE) for details.

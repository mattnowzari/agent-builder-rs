# Importing Components

The TUI supports importing custom tools and skills from local YAML files or directly from a GitHub repository, and installing plugins from a URL. This document covers the YAML format for each component type, expected folder layout, and how to perform imports.

## How to Import (Local File)

1. Navigate to the **Components** panel (use `Tab` to cycle panels)
2. Switch to the desired tab (**Tools** or **Skills**) using `◀` / `▶`
3. Press `i` to open the file explorer
4. Navigate to the `.yaml` or `.yml` file and press `Enter`
5. The TUI will parse the YAML, call the Kibana Agent Builder API, and display a success or error modal

---

## Tool Import

A tool is defined as a **single YAML file**. Each tool type (`esql`, `index_search`, `workflow`) has its own set of configuration fields.

### Common Fields

| Field         | Type     | Required | Description                                  |
|---------------|----------|----------|----------------------------------------------|
| `id`          | string   | Yes      | Unique tool identifier                       |
| `type`        | string   | Yes      | One of: `esql`, `index_search`, `workflow`   |
| `description` | string   | No       | What the tool does                           |
| `tags`        | string[] | No       | Tags for categorization                      |

### ES|QL Tool

Runs an ES|QL query. Supports parameterized queries using `?param_name` placeholders.

```yaml
id: esql-user-lookup
type: esql
description: "Look up users by email domain and return recent login activity"
tags:
  - users
  - security
query: |
  FROM logs-system.auth-*
  | WHERE user.email LIKE ?domain
  | STATS login_count = COUNT(*), last_seen = MAX(@timestamp) BY user.name
  | SORT last_seen DESC
  | LIMIT ?max_results
params:
  domain:
    type: string
    description: "Email domain pattern to filter by (e.g. *@elastic.co)"
  max_results:
    type: integer
    description: "Maximum number of users to return"
    optional: true
    default_value: 25
```

**ES|QL-specific fields:**

| Field    | Type   | Required | Description                                   |
|----------|--------|----------|-----------------------------------------------|
| `query`  | string | Yes      | The ES\|QL query (use `\|` for YAML multiline) |
| `params` | map    | No       | Named parameters the agent can fill at runtime |

Each param entry supports:

| Field           | Type    | Required | Description                     |
|-----------------|---------|----------|---------------------------------|
| `type`          | string  | Yes      | Parameter type (`string`, `integer`, etc.) |
| `description`   | string  | No       | What this parameter controls    |
| `optional`      | boolean | No       | Whether the param can be omitted (default: `false`) |
| `default_value` | any     | No       | Default value when omitted      |

### Index Search Tool

Searches an Elasticsearch index pattern.

```yaml
id: index-search-security-logs
type: index_search
description: "Search security event logs for suspicious authentication activity"
tags:
  - security
  - logs
pattern: "logs-security.*"
row_limit: 100
custom_instructions: |
  Focus on failed authentication attempts and privilege escalation events.
  Prioritize entries from the last 24 hours.
```

**Index search-specific fields:**

| Field                 | Type   | Required | Description                          |
|-----------------------|--------|----------|--------------------------------------|
| `pattern`             | string | Yes      | Elasticsearch index pattern          |
| `row_limit`           | number | No       | Maximum rows to return               |
| `custom_instructions` | string | No       | Additional instructions for the agent |

### Workflow Tool

Triggers a pre-defined workflow.

```yaml
id: workflow-alert-enrichment
type: workflow
description: "Enrich security alerts with threat intel and asset context"
tags:
  - security
  - automation
workflow_id: "enrich-security-alerts"
wait_for_completion: true
```

**Workflow-specific fields:**

| Field                  | Type    | Required | Description                              |
|------------------------|---------|----------|------------------------------------------|
| `workflow_id`          | string  | Yes      | ID of the workflow to execute            |
| `wait_for_completion`  | boolean | No       | Wait for the workflow to finish (default: `false`) |

---

## Skill Import

A skill is defined as a **folder** containing a YAML definition file and one or more markdown files. The YAML references the markdown files via relative paths — the actual content lives in `.md` files, not inline in the YAML.

### Folder Layout

```
skills/
  my-skill/
    my-skill.yaml           # Skill definition (this is the file you select when importing)
    my-skill.md              # Main skill instructions (markdown)
    runbook.md               # Optional: referenced content file
    additional-context.md    # Optional: another referenced content file
```

The YAML file and all referenced `.md` files must live in the same directory (or subdirectories reachable via the relative paths in the YAML).

### YAML Schema

```yaml
id: my-skill
name: My Skill
description: "What this skill does"
content: ./my-skill.md
referenced_content:
  - name: runbook
    path: ./runbook.md
  - name: additional-context
    path: ./additional-context.md
tool_ids:
  - esql-user-lookup
```

| Field                | Type     | Required | Description                                                      |
|----------------------|----------|----------|------------------------------------------------------------------|
| `id`                 | string   | Yes      | Unique identifier. Lowercase alphanumeric, hyphens, underscores. 1-64 chars. |
| `name`               | string   | Yes      | Display name. 1-64 chars.                                        |
| `description`        | string   | Yes      | What the skill does. 1-1024 chars.                               |
| `content`            | string   | Yes      | Relative path to the main markdown file.                         |
| `referenced_content` | array    | No       | Additional markdown files bundled with the skill (max 100).      |
| `tool_ids`           | string[] | No       | Tool IDs this skill references (max 5).                          |

Each `referenced_content` entry:

| Field  | Type   | Required | Description                                        |
|--------|--------|----------|----------------------------------------------------|
| `name` | string | Yes      | Display name for this content (1-64 chars).        |
| `path` | string | Yes      | Relative path to the markdown file.                |

### How Content Resolution Works

When you import a skill, the TUI:

1. Reads the YAML file you selected
2. Resolves `content` relative to the YAML file's parent directory and reads the `.md` file
3. For each `referenced_content` entry, resolves `path` the same way and reads the `.md` file
4. Sends the full content (markdown inlined) to the Kibana Agent Builder API

The `relativePath` sent to the API for each referenced content entry is automatically set to `./<filename>` (Kibana's internal convention). You don't need to worry about this — just use local relative paths in your YAML.

### Full Example

**`skills/incident-response/incident-response.yaml`:**

```yaml
id: incident-response
name: Incident Response
description: "Guides the agent through structured incident response procedures for security events"
content: ./incident-response.md
referenced_content:
  - name: runbook
    path: ./runbook.md
tool_ids:
  - esql-user-lookup
```

**`skills/incident-response/incident-response.md`:**

```markdown
# Incident Response Skill

You are an incident response specialist. When the user reports a security
incident, follow these steps:

## Step 1: Triage
Assess the severity and scope...

## Step 2: Investigation
Use available tools to gather evidence...

## Step 3: Containment Recommendations
Based on your findings, recommend containment actions...
```

**`skills/incident-response/runbook.md`:**

```markdown
# Incident Response Runbook

## Escalation Contacts

| Severity | Contact | SLA |
|----------|---------|-----|
| Critical | SOC Lead | 15 minutes |
| High | On-call Security Engineer | 1 hour |
...
```

---

## Plugin Install

A plugin is a **ZIP archive** that bundles skills and metadata following the [Claude agent plugin specification](https://code.claude.com/docs/en/plugins). Unlike tools and skills, plugins are installed from a **URL** (a GitHub repository or a direct link to a ZIP file) rather than from a local file.

### Prerequisites

Plugin install requires the `agentBuilder:experimentalFeatures` Kibana advanced setting to be enabled. If the setting is disabled, the install endpoint returns `404`.

### Archive Layout

A plugin ZIP contains a manifest and one or more skill directories:

```
my-plugin/
├── manifest.yaml          # Plugin metadata (name, version, description, ...)
├── skills/
│   ├── my-skill/
│   │   ├── SKILL.md       # Skill instructions (YAML frontmatter + markdown)
│   │   └── helper.md      # Optional referenced content
│   └── another-skill/
│       └── SKILL.md
```

The `manifest.yaml` (or `manifest.json`) must include at least a `name` field:

```yaml
name: My Plugin
version: 1.0.0
description: "A plugin that adds analysis skills"
author:
  name: Author Name
  email: author@example.com
skills:
  - skills/*/
```

Each skill directory must contain a `SKILL.md` file. YAML frontmatter in `SKILL.md` provides the skill's `id`, `name`, and `description`. Any sibling files are automatically collected as referenced content.

### How to Install

1. Navigate to the **Components** panel (use `Tab` to cycle panels)
2. Switch to the **Plugins** tab using `◀` / `▶`
3. Press `i` to open the install dialog
4. Enter a GitHub repository URL (e.g. `https://github.com/org/my-plugin`) or a direct ZIP URL
5. Press `Enter` to install

The Kibana Agent Builder API downloads the archive, parses the manifest, extracts skills, and persists everything. On success, a confirmation modal shows the installed plugin name. All extracted skills appear in the Skills tab and can be assigned to agents.

### Supported URL Formats

- **GitHub repository:** `https://github.com/org/my-agent-plugin` — Kibana fetches the archive from the repository
- **Direct ZIP URL:** `https://example.com/path/to/plugin.zip` — any publicly accessible ZIP

### What Happens on Install

1. The archive is downloaded from the URL by the Kibana server
2. The manifest is parsed and validated
3. Skill directories are scanned for `SKILL.md` files with optional YAML frontmatter
4. Each skill is persisted individually with a `plugin_id` linking it back to the plugin
5. The plugin document is created with references to all extracted skill IDs

### Notes

- Plugin IDs are auto-generated UUIDs — you don't specify them
- Plugin names must be unique across all installed plugins
- Deleting a plugin also deletes all skills it installed
- The URL must be accessible from the Kibana server (not from your local machine)

---

## Importing from GitHub

Tools and skills can be imported directly from a **public** GitHub repository using the `g` keybinding. The TUI fetches raw file contents via `raw.githubusercontent.com`, so no GitHub token is needed for public repos.

### How to Import from GitHub

1. Navigate to the **Components** panel (use `Tab` to cycle panels)
2. Switch to the desired tab (**Tools** or **Skills**) using `◀` / `▶`
3. Press `g` to open the GitHub import dialog
4. Paste a GitHub file or folder URL and press `Enter`
5. The TUI will fetch the YAML (and any referenced markdown files for skills), create the component via the Kibana API, and display a success or error modal

> For **Plugins**, pressing `g` opens the same URL install dialog as `i` (Kibana fetches plugin archives server-side).

### Supported URL Formats

The `g` dialog accepts standard GitHub URLs — just copy the URL from your browser when viewing the file or folder on GitHub.

| Component | URL Pattern | Example |
|-----------|-------------|---------|
| **Tool** | `/blob/` (single file) | `https://github.com/org/repo/blob/main/tools/esql-user-lookup.yaml` |
| **Skill** | `/blob/` (YAML file) | `https://github.com/org/repo/blob/main/skills/my-skill/my-skill.yaml` |
| **Skill** | `/tree/` (folder) | `https://github.com/org/repo/tree/main/skills/my-skill` |

Both `https://github.com/...` and `http://github.com/...` are accepted. The URL must contain either `/blob/` (pointing to a file) or `/tree/` (pointing to a directory).

### Skill Folder Convention

When you provide a `/tree/` (folder) URL for a skill, the TUI derives the YAML filename by convention: it looks for a file named `<folder-name>.yaml` inside the folder. For example:

- URL: `https://github.com/org/repo/tree/main/skills/my-skill`
- Expected YAML: `skills/my-skill/my-skill.yaml`

The `content` and `referenced_content` paths in the YAML are then resolved relative to that YAML file's location in the repo, just like local imports.

Alternatively, you can point directly at the YAML file using a `/blob/` URL — this skips the convention-based lookup.

### What Happens on GitHub Import

1. The GitHub URL is parsed to extract `owner`, `repo`, `ref` (branch or tag), and `path`
2. The raw URL is constructed: `https://raw.githubusercontent.com/{owner}/{repo}/{ref}/{path}`
3. The YAML file is fetched and parsed
4. **For tools:** the parsed YAML is sent to the Kibana Agent Builder API via `create_tool`
5. **For skills:** the `content` markdown file and any `referenced_content` markdown files are also fetched from the same repo (paths resolved relative to the YAML), then the full payload is sent to the Kibana API via `create_skill`

### GitHub Import Examples

**Importing a tool from GitHub:**

```
https://github.com/myorg/security-tools/blob/main/tools/esql-user-lookup.yaml
```

The TUI fetches the single YAML file and creates the tool.

**Importing a skill folder from GitHub:**

```
https://github.com/myorg/security-tools/tree/main/skills/incident-response
```

The TUI:
1. Derives the YAML path → `skills/incident-response/incident-response.yaml`
2. Fetches and parses the YAML
3. Fetches `skills/incident-response/incident-response.md` (the `content` file)
4. Fetches `skills/incident-response/runbook.md` (a `referenced_content` file)
5. Creates the skill with all content inlined

**Importing a skill YAML directly:**

```
https://github.com/myorg/security-tools/blob/v2.0/skills/incident-response/incident-response.yaml
```

Same as above, but uses the explicit `/blob/` URL and a tag (`v2.0`) instead of a branch.

### Notes

- **Public repos only** — private repositories require authentication, which is not currently supported. The TUI fetches files anonymously from `raw.githubusercontent.com`.
- **Branch, tag, or commit** — the `ref` segment in the URL can be a branch name (`main`), a tag (`v1.0`), or a full commit SHA.
- **Tool URLs must use `/blob/`** — tools are single files, so folder (`/tree/`) URLs are not accepted for tool imports.
- **Same YAML format** — the YAML schema is identical whether you import from a local file or from GitHub. See the [Tool Import](#tool-import) and [Skill Import](#skill-import) sections above for the full schema reference.

---

## ID Conventions

Both tool and skill IDs follow the same pattern:

- Lowercase letters, numbers, hyphens, and underscores only
- Must start and end with a letter or number
- 1-64 characters
- Pattern: `^[a-z0-9](?:[a-z0-9_-]*[a-z0-9])?$`

For tools, the convention is to prefix with the type: `esql-*`, `index-search-*`, `workflow-*`.

# Importing Tools and Skills

The TUI supports importing custom tools and skills from YAML definition files. This document covers the YAML format for each component type, expected folder layout, and how to perform an import.

## How to Import

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

## ID Conventions

Both tool and skill IDs follow the same pattern:

- Lowercase letters, numbers, hyphens, and underscores only
- Must start and end with a letter or number
- 1-64 characters
- Pattern: `^[a-z0-9](?:[a-z0-9_-]*[a-z0-9])?$`

For tools, the convention is to prefix with the type: `esql-*`, `index-search-*`, `workflow-*`.

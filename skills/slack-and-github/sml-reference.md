# SML Quick Reference for Connector Discovery

This is a condensed reference for the Semantic Metadata Layer (SML) tools used during connector discovery and execution.

## Tools

### sml_search

Queries the SML index to discover Kibana assets (connectors, visualizations, etc.).

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `query` | string (1-512 chars) | Yes | Search text. Use `"*"` to return all assets. |
| `size` | number (1-50) | No | Max results (default: 10) |

Returns a list of results, each containing:

| Field | Description |
|-------|-------------|
| `chunk_id` | SML document ID — pass this to `sml_attach` |
| `attachment_id` | Origin ID (e.g., connector saved object ID) |
| `attachment_type` | SML type: `connector`, `visualization`, etc. |
| `title` | Display title |
| `content` | Searchable content text |
| `score` | Relevance score |

### sml_attach

Converts SML search results into conversation attachments.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `chunk_ids` | string[] (1-50) | Yes | Chunk IDs from `sml_search` results |

After attaching, the connector's metadata (including sub-actions) becomes available in the system prompt **on the next turn**.

### execute_connector_sub_action

Executes a sub-action on an attached connector.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `connectorId` | string | Yes | Connector ID from the attachment |
| `subAction` | string | Yes | Exact sub-action name from the attachment catalog |
| `params` | object | No | Parameters for the sub-action |

**You MUST read the connector attachment before calling this tool.** The attachment contains the `connectorId`, available sub-actions, and their parameter schemas. Never guess.

## The Two-Turn Constraint

Attachments created during a turn are not visible in that same turn's system prompt. The system prompt is built **once** at the start of each turn, before any tools run.

| Turn | What happens |
|------|-------------|
| Turn N | `sml_search` finds connector → `sml_attach` creates attachment → attachment exists but is **not** in system prompt |
| Turn N+1 | `prepareConversation()` formats the attachment → sub-action catalog appears in system prompt → `execute_connector_sub_action` works |

This is not an error. It is how the attachment lifecycle works.

## Connector Types

| Service | `connector_type` value |
|---------|----------------------|
| GitHub | `.github` |
| Slack (API) | `.slack_api` |
| Jira Cloud | `.jira` |
| Email | `.email` |
| ServiceNow ITSM | `.servicenow` |
| PagerDuty | `.pagerduty` |
| Webhook | `.webhook` |
| Microsoft Teams | `.teams` |

## Prerequisites

For SML tools to work, the Kibana deployment must have:

1. `agentBuilder:experimentalFeatures` enabled in Kibana Advanced Settings
2. Connectors created (or re-saved) **after** enabling experimental features
3. The agent configured with `enable_elastic_capabilities: true` (includes SML tools by default)
4. The user's API key must have `action:execute` privileges

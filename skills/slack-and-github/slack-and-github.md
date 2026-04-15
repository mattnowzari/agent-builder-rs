# Slack & GitHub Connector Skill

You help users interact with Slack and GitHub through Kibana connectors. Users will ask questions like "post a message to #general", "create a GitHub issue", or "who am I on GitHub?" — they will **not** provide connector IDs, chunk IDs, or technical SML details. Your job is to handle all of that behind the scenes.

## When This Skill Applies

Activate this workflow whenever the user's request involves:

- **Slack:** sending messages, listing channels, reading messages, managing channels
- **GitHub:** creating/reading issues, pull requests, repositories, checking identity, commenting

## Connector Discovery Workflow

The user does not have access to Kibana's UI and cannot provide chunk IDs or connector IDs. You must discover and attach the connectors yourself using the Semantic Metadata Layer (SML). Refer to the SML reference for full details on the tools and attachment lifecycle.

### Step 1: Search for the Connector

Use `sml_search` to find the relevant connector. Choose your query based on what the user is asking for:

- For Slack requests → search for `"slack"` or `"slack_api"`
- For GitHub requests → search for `"github"`
- If both are needed → run two searches (or a single broad search like `"slack github"`)

Examine the results. Each result includes a `chunk_id`, `attachment_type`, and `title`. Confirm the result is a `connector` type before proceeding.

If `sml_search` returns zero results:
- The connector may not exist in this Kibana deployment
- The SML experimental features setting may be disabled
- The connector may have been created before experimental features were enabled and needs to be re-saved
- Tell the user clearly what happened and what they can do about it

### Step 2: Attach the Connector

Call `sml_attach` with the `chunk_id`(s) from the search results. This registers the connector as a conversation attachment.

**Critical — the two-turn constraint:** When you discover and attach a connector within the same turn, the connector's sub-action catalog is **not** available to you until the next turn. This is a platform limitation, not an error. The attachment metadata is injected into your system prompt at the start of each turn, so attachments created mid-turn are invisible until the conversation continues.

After attaching, tell the user the connector was found and attached, and ask them to send a follow-up message so you can proceed. Be direct:

> "I found and attached the [Slack/GitHub] connector. Send me your request again (or just say 'go ahead') and I'll execute it."

Do **not** attempt to guess sub-action names or parameters on the attachment turn. You will get them wrong.

### Step 3: Execute the Sub-Action

On the follow-up turn, the connector attachment is now in your system prompt. It contains:

- The `connectorId`
- The `connector_type` (`.slack_api`, `.github`, etc.)
- A full catalog of available sub-actions with their parameter schemas

Read this attachment carefully. Use `execute_connector_sub_action` with:
- `connectorId` — from the attachment
- `subAction` — the **exact** sub-action name from the catalog (do not guess)
- `params` — matching the parameter schema from the catalog

### Handling Requests That Need Both Connectors

If the user asks something like "summarize my GitHub issues and post the summary to Slack":

1. Search for and attach **both** connectors in one turn
2. Ask the user to continue
3. On the next turn, orchestrate calls to both: first fetch from GitHub, then send to Slack

## Common Patterns

### Slack

| User intent | Sub-action | Key params |
|-------------|-----------|------------|
| Send a message | `postMessage` | `channel`, `text` |
| List channels | `channels` | — |
| Get channel history | Not available as a tool sub-action on all setups — check the catalog |

### GitHub

| User intent | Sub-action | Key params |
|-------------|-----------|------------|
| Who am I? | `getMe` | — |
| Create an issue | `createIssue` | `owner`, `repo`, `title`, `body` |
| Get an issue | `getIssue` | `owner`, `repo`, `issue_number` |
| List issues | Not always available — check the catalog |
| Create a comment | `createComment` | `owner`, `repo`, `issue_number`, `body` |

These are common sub-actions but the actual availability depends on the connector spec in your deployment. **Always read the attachment catalog** rather than relying on this table.

## Error Handling

- **"No connector spec found"** → The `connector_type` is wrong. Ask the user to verify the connector setup in Kibana.
- **Permission denied** → The user's API key lacks `action:execute` privileges. They need to update their API key permissions.
- **Sub-action not found** → You used a sub-action name that doesn't exist. Re-read the attachment catalog and use an exact match.
- **Empty SML results** → See Step 1 troubleshooting above.

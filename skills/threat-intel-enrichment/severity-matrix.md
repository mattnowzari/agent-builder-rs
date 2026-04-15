# Threat Severity Matrix

## Risk Classification

| Level | Score | Criteria | Response SLA |
|-------|-------|----------|--------------|
| Critical | 9-10 | Active exploitation, known APT infrastructure, confirmed data breach | Immediate (< 15 min) |
| High | 7-8 | Multiple threat feed hits, recent activity, associated with known campaigns | 1 hour |
| Medium | 4-6 | Single threat feed hit, older activity, no confirmed exploitation | 4 hours |
| Low | 1-3 | Suspicious but unconfirmed, low confidence indicators | Next business day |
| Informational | 0 | No threat data found, benign classification | No action required |

## Scoring Factors

### Increases severity (+1 to +3)
- Observable seen in multiple independent threat feeds
- Associated with a named threat actor or campaign
- Active within the last 7 days in the environment
- Connected to a known vulnerability (CVE)
- Targets critical infrastructure or sensitive data stores

### Decreases severity (-1 to -3)
- Only seen in a single low-confidence feed
- Last activity > 90 days ago
- Associated with legitimate dual-use tools (not inherently malicious)
- Observable belongs to a known CDN or cloud provider
- No correlated alerts or detections in the environment

## Confidence Levels

| Confidence | Description |
|-----------|-------------|
| High | 3+ independent sources agree, recent corroboration |
| Medium | 1-2 sources, some corroboration |
| Low | Single source, no corroboration, aged data |

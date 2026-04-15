# Incident Response Runbook

## Escalation Contacts

| Severity | Contact | SLA |
|----------|---------|-----|
| Critical | Security Operations Center (SOC) Lead | 15 minutes |
| High | On-call Security Engineer | 1 hour |
| Medium | Security Team channel | 4 hours |
| Low | Security Team backlog | Next business day |

## Standard Procedures

### Account Compromise

1. Disable the compromised account immediately
2. Revoke all active sessions and API keys
3. Audit recent actions taken by the account
4. Notify the account owner through an out-of-band channel
5. Re-enable only after password reset and MFA verification

### Malware Detection

1. Isolate the affected host from the network
2. Capture a memory dump if possible
3. Collect relevant log artifacts
4. Run a full endpoint scan
5. Check for lateral movement indicators

### Data Exfiltration

1. Identify the data classification level
2. Block the exfiltration channel (IP, domain, or service)
3. Assess the volume and sensitivity of data involved
4. Engage Legal and Compliance if PII or regulated data is affected
5. Preserve all evidence for forensic analysis

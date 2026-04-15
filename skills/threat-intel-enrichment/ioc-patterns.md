# IOC Pattern Reference

## Common Indicator Formats

### IP Addresses
- IPv4: `192.168.1.1`
- IPv6: `2001:0db8:85a3::8a2e:0370:7334`
- CIDR notation: `10.0.0.0/8`

### Domains
- Bare domain: `example.com`
- Subdomain: `malware.evil.example.com`
- Defanged: `example[.]com`, `hxxps://evil[.]com`

### File Hashes
- MD5: 32 hex characters
- SHA-1: 40 hex characters
- SHA-256: 64 hex characters

### URLs
- Full URL: `https://example.com/path?query=value`
- Defanged: `hxxps://example[.]com/payload`

## Known Malicious Patterns

### DGA (Domain Generation Algorithm) Indicators
- High entropy domain names (e.g., `xkjf8a3ndf.com`)
- Recently registered domains (< 30 days)
- Domains with no meaningful WHOIS information

### C2 Beaconing Patterns
- Regular interval connections (e.g., every 60 seconds)
- Low-volume, consistent data transfer
- Connections to unusual ports (e.g., 8443, 4443)
- Use of uncommon TLDs

### Data Exfiltration Indicators
- Large outbound transfers to new destinations
- DNS tunneling (unusually long subdomain labels)
- Connections to cloud storage APIs from unexpected hosts

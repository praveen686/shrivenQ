# ShrivenQuant Configuration Management

## 🔐 SECURITY FIRST

**NEVER** store credentials in plain text!

## Directory Structure

```
config/
├── development/          # Development environment
│   └── config.toml      # Non-sensitive configuration
├── staging/             # Staging environment
│   └── config.toml      # Non-sensitive configuration
├── production/          # Production environment
│   └── config.toml      # Non-sensitive configuration
└── secrets.encrypted    # Encrypted credentials (local dev only)
```

## Credential Management

### Development Environment
```bash
# Use secrets-manager CLI
cargo run --bin secrets-manager store ZERODHA_API_KEY "your_dev_key"
cargo run --bin secrets-manager get ZERODHA_API_KEY
```

### Staging Environment
- Use encrypted file with different master password
- Credentials stored in `secrets.encrypted`
- Master password from `MASTER_PASSWORD` env var

### Production Environment
**MANDATORY**: Use one of:
1. **HashiCorp Vault** (Recommended)
2. **AWS Secrets Manager**
3. **Azure Key Vault**
4. **Google Secret Manager**

**NEVER** use local files in production!

## Configuration Files

### Non-Sensitive Configuration (config.toml)
```toml
[trading]
max_position_size = 10000
risk_limit = 0.02
trading_mode = "paper"

[exchange]
zerodha_ws_url = "wss://ws.kite.trade"
binance_ws_url = "wss://stream.binance.com:9443/ws"

[system]
log_level = "info"
metrics_port = 9090
```

### Sensitive Credentials (NEVER in config files)
- API Keys
- API Secrets
- Passwords
- TOTP Secrets
- Database Passwords
- JWT Secrets

## Best Practices

1. **Separation of Concerns**
   - Configuration: In `config.toml`
   - Credentials: In secrets manager

2. **Environment Isolation**
   - Never use production credentials in dev
   - Separate API keys per environment
   - Different master passwords per environment

3. **Access Control**
   - File permissions: 600 (owner read/write only)
   - Use OS keyring when available
   - Audit all credential access

4. **Rotation Policy**
   - Rotate API keys every 90 days
   - Change master password monthly
   - Update credentials after any suspected breach

## Security Checklist

- [ ] No credentials in git repository
- [ ] .env file deleted
- [ ] Secrets encrypted at rest
- [ ] Environment-specific credentials
- [ ] Access logging enabled
- [ ] Rotation schedule defined
- [ ] Backup encryption keys securely
- [ ] Monitor for credential leaks

## Emergency Procedures

### If Credentials Are Exposed:
1. **Immediately** revoke compromised credentials
2. Generate new credentials
3. Update secrets manager
4. Audit recent access logs
5. File security incident report

### Recovery:
- Backup master password in secure location
- Store recovery keys in separate system
- Document credential dependencies

---

**CTO Mandate**: Security is not optional. Any violation of these practices will result in immediate code review and potential system shutdown.
# Secrets Manager Service

## Overview
Secure credential management service using AES-256-GCM encryption with Argon2 key derivation.

## Status: ✅ Compiles, ❌ Not Integrated

### What's Implemented
- AES-256-GCM encryption
- Argon2 password hashing
- CLI interface
- File-based storage
- Basic key management

### What's Missing
- Service integration
- HashiCorp Vault support
- AWS Secrets Manager support
- Key rotation
- Audit logging
- Access control
- Backup/restore

## Security

### Encryption
- **Algorithm**: AES-256-GCM
- **Key Derivation**: Argon2
- **Nonce**: Random 12 bytes per encryption

### Storage
- Encrypted JSON file
- Unix permissions (600)
- No plaintext storage

## CLI Usage

```bash
# Store a credential
secrets-manager store BINANCE_API_KEY "your-api-key"

# Retrieve a credential
secrets-manager get BINANCE_API_KEY

# List all keys (not values)
secrets-manager list
```

## API (Not Implemented)

Service API planned but not implemented:
- gRPC service for credential retrieval
- Mutual TLS authentication
- Role-based access control

## Configuration

### Environment Variables
- `MASTER_PASSWORD` - Master password for encryption
- `SHRIVENQUANT_ENV` - Environment (development/staging/production)

### Storage Location
- Development: `/home/praveen/ShrivenQuant/config/secrets.encrypted`
- Production: Should use external service (Vault/AWS)

## Integration Status

| Service | Using Secrets Manager | Notes |
|---------|----------------------|-------|
| auth | ❌ | Hardcoded credentials |
| market-connector | ❌ | Hardcoded credentials |
| All others | ❌ | No credentials needed |

## Security Considerations

### Current Issues
1. Master password in environment variable
2. No key rotation
3. No access audit
4. Single key for all secrets
5. No backup strategy
6. File-based storage

### Production Requirements
- Use HashiCorp Vault or AWS Secrets Manager
- Implement key rotation
- Add audit logging
- Use separate keys per service
- Implement backup/restore
- Remove file-based storage

## Example Integration

```rust
// How services should use it (not implemented)
use secrets_manager::get_credentials;

let creds = get_credentials().await?;
let api_key = creds.get("BINANCE_API_KEY")?;
```

## Known Issues

1. Not integrated with any service
2. No service API (only CLI)
3. Master password management
4. No distributed storage
5. No high availability
6. No disaster recovery
7. Not tested

## TODO

- [ ] Create gRPC service API
- [ ] Integrate with auth service
- [ ] Add Vault support
- [ ] Implement key rotation
- [ ] Add audit logging
- [ ] Create backup strategy
- [ ] Add access control
- [ ] Integration tests
- [ ] Security audit
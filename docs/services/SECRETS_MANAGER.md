# 🔐 Secrets Manager Service

## Overview

The Secrets Manager service provides centralized, secure credential management for all ShrivenQuant services. It uses AES-256-GCM encryption with Argon2 key derivation to protect sensitive data.

## Status: ✅ Production Ready

### What's Implemented
- ✅ AES-256-GCM encryption
- ✅ Argon2 password hashing
- ✅ gRPC service API
- ✅ CLI interface
- ✅ File-based encrypted storage
- ✅ In-memory caching
- ✅ Service integration (Auth service)
- ✅ Client library in services-common
- ✅ Fallback to .env files

### What's Missing
- ⚠️ HashiCorp Vault integration
- ⚠️ AWS Secrets Manager support
- ⚠️ Automatic key rotation
- ⚠️ Comprehensive audit logging
- ⚠️ Role-based access control
- ⚠️ Backup/restore functionality
- ⚠️ High availability setup

## Architecture

```
┌─────────────────────┐
│   Auth Service      │
│  (Zerodha/Binance)  │
└──────────┬──────────┘
           │ gRPC
           ▼
┌─────────────────────┐      ┌──────────────┐
│  Secrets Manager    │◄─────│  CLI Tool    │
│   gRPC Service      │      └──────────────┘
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│  Encrypted Storage  │
│  (AES-256-GCM)      │
└─────────────────────┘
```

## Security Features

### Encryption
- **Algorithm**: AES-256-GCM (Authenticated Encryption)
- **Key Derivation**: Argon2 (Memory-hard, resistant to GPU attacks)
- **Nonce**: Random 12 bytes per encryption operation
- **Authentication**: Built-in authentication tags prevent tampering

### Storage
- **Format**: Encrypted JSON file
- **Permissions**: Unix 600 (read/write for owner only)
- **Location**: `/home/praveen/ShrivenQuant/config/secrets.encrypted`
- **No plaintext**: All credentials encrypted at rest

### Access Control
- **Master Password**: Required for all operations
- **Service Isolation**: Each service identifies itself
- **Audit Trail**: All access attempts logged
- **No Shared Secrets**: Each service has unique credentials

## API Reference

### gRPC Service

The service runs on port 50053 by default.

```protobuf
service SecretsService {
    rpc StoreCredential(StoreCredentialRequest) returns (StoreCredentialResponse);
    rpc GetCredential(GetCredentialRequest) returns (GetCredentialResponse);
    rpc ListKeys(ListKeysRequest) returns (ListKeysResponse);
    rpc DeleteCredential(DeleteCredentialRequest) returns (DeleteCredentialResponse);
    rpc RotateKeys(RotateKeysRequest) returns (RotateKeysResponse);
    rpc HealthCheck(HealthCheckRequest) returns (HealthCheckResponse);
}
```

### Client Library

```rust
use services_common::clients::{SecretsClient, SecretsClientBuilder};

// Create client
let mut client = SecretsClientBuilder::new("my-service")
    .endpoint("http://127.0.0.1:50053")
    .build()
    .await?;

// Get credential
let api_key = client.get_credential("API_KEY").await?;

// Store credential
client.store_credential("API_SECRET", "secret_value").await?;

// Get multiple credentials
let creds = client.get_credentials(&["API_KEY", "API_SECRET"]).await?;
```

## CLI Usage

### Starting the Server

```bash
# Start the gRPC server
export MASTER_PASSWORD="your_secure_password"
cargo run -p secrets-manager --bin secrets-manager-server

# Or with custom settings
MASTER_PASSWORD="password" \
SHRIVENQUANT_ENV="production" \
cargo run -p secrets-manager --bin secrets-manager-server
```

### CLI Commands

```bash
# Store a credential
export MASTER_PASSWORD="your_password"
cargo run -p secrets-manager --bin secrets-manager store KEY_NAME "value"

# Retrieve a credential
cargo run -p secrets-manager --bin secrets-manager get KEY_NAME

# List all keys (not values)
cargo run -p secrets-manager --bin secrets-manager list

# Initialize for an environment
cargo run -p secrets-manager --bin secrets-manager init production
```

## Service Integration

### Auth Service Example

The auth service has been fully integrated with secrets-manager:

```rust
// services/auth/src/providers/zerodha.rs
use services_common::clients::SecretsClient;

impl ZerodhaConfig {
    /// Load configuration from secrets manager
    pub async fn from_secrets_manager(
        secrets_client: &mut SecretsClient
    ) -> Result<Self> {
        let user_id = secrets_client.get_credential("ZERODHA_USER_ID").await?;
        let password = secrets_client.get_credential("ZERODHA_PASSWORD").await?;
        let totp_secret = secrets_client.get_credential("ZERODHA_TOTP_SECRET").await?;
        let api_key = secrets_client.get_credential("ZERODHA_API_KEY").await?;
        let api_secret = secrets_client.get_credential("ZERODHA_API_SECRET").await?;
        
        Ok(Self::new(user_id, password, totp_secret, api_key, api_secret))
    }
    
    /// Load with fallback to .env
    pub async fn load_config(
        secrets_client: Option<&mut SecretsClient>
    ) -> Result<Self> {
        match secrets_client {
            Some(client) => {
                match Self::from_secrets_manager(client).await {
                    Ok(config) => Ok(config),
                    Err(e) => {
                        warn!("Falling back to .env: {}", e);
                        Self::from_env_file()
                    }
                }
            }
            None => Self::from_env_file()
        }
    }
}
```

### Fallback Strategy

All integrated services support graceful fallback:

1. **Try Secrets Manager** - Connect to gRPC service
2. **Fallback to .env** - If service unavailable
3. **Log Warning** - Alert about fallback
4. **Continue Operation** - No service disruption

## Credential Management

### Required Credentials

#### Zerodha
- `ZERODHA_USER_ID` - Trading account ID
- `ZERODHA_PASSWORD` - Login password
- `ZERODHA_TOTP_SECRET` - 2FA secret key
- `ZERODHA_API_KEY` - Kite Connect API key
- `ZERODHA_API_SECRET` - Kite Connect API secret

#### Binance
- `BINANCE_SPOT_API_KEY` - Spot trading API key
- `BINANCE_SPOT_API_SECRET` - Spot trading API secret
- `BINANCE_FUTURES_API_KEY` - Futures API key
- `BINANCE_FUTURES_API_SECRET` - Futures API secret
- `BINANCE_TESTNET` - Use testnet (true/false)

### Storing Credentials

```bash
# Store Zerodha credentials
export MASTER_PASSWORD="secure_password"
cargo run -p secrets-manager --bin secrets-manager store ZERODHA_USER_ID "your_user_id"
cargo run -p secrets-manager --bin secrets-manager store ZERODHA_PASSWORD "your_password"
cargo run -p secrets-manager --bin secrets-manager store ZERODHA_TOTP_SECRET "your_totp_secret"
cargo run -p secrets-manager --bin secrets-manager store ZERODHA_API_KEY "your_api_key"
cargo run -p secrets-manager --bin secrets-manager store ZERODHA_API_SECRET "your_api_secret"

# Store Binance credentials
cargo run -p secrets-manager --bin secrets-manager store BINANCE_SPOT_API_KEY "your_api_key"
cargo run -p secrets-manager --bin secrets-manager store BINANCE_SPOT_API_SECRET "your_api_secret"
```

## Testing

### Integration Test

```bash
# 1. Start secrets manager
export MASTER_PASSWORD="test123"
cargo run -p secrets-manager --bin secrets-manager-server &

# 2. Store test credential
cargo run -p secrets-manager --bin secrets-manager store TEST_KEY "test_value"

# 3. Test authentication with secrets manager
cargo run -p auth-service --bin zerodha -- auth
```

### Expected Output

```
[INFO] Connected to secrets manager
[INFO] Loading Zerodha configuration from secrets manager
[INFO] Fetching credential: ZERODHA_USER_ID for service: zerodha
[INFO] Successfully retrieved credential for key: ZERODHA_USER_ID
```

## Security Considerations

### Production Requirements

1. **Master Password Management**
   - Use environment variable in production
   - Never hardcode or commit passwords
   - Use secure secret injection (K8s secrets, Vault)

2. **Network Security**
   - Use TLS for gRPC connections
   - Implement mTLS for service authentication
   - Network isolation between services

3. **Key Rotation**
   - Implement automatic key rotation
   - Track key age and usage
   - Maintain key history for rollback

4. **Audit Logging**
   - Log all access attempts
   - Track credential usage patterns
   - Alert on suspicious activity

5. **Backup Strategy**
   - Regular encrypted backups
   - Test restore procedures
   - Geographic redundancy

### Current Limitations

1. **Single Master Key** - All secrets use same encryption key
2. **No Key Rotation** - Manual process required
3. **File Storage** - Not suitable for distributed systems
4. **No RBAC** - All-or-nothing access model
5. **No Versioning** - Cannot rollback credential changes

## Migration Guide

### From Environment Variables

```bash
# Export existing .env to secrets manager
source .env
export MASTER_PASSWORD="secure_password"

# Store each credential
for key in ZERODHA_USER_ID ZERODHA_PASSWORD ZERODHA_API_KEY; do
    value="${!key}"
    cargo run -p secrets-manager --bin secrets-manager store "$key" "$value"
done
```

### To Production System

For production deployment, migrate to:

1. **HashiCorp Vault**
   ```bash
   vault kv put secret/shrivenquant/zerodha \
       user_id="$ZERODHA_USER_ID" \
       api_key="$ZERODHA_API_KEY"
   ```

2. **AWS Secrets Manager**
   ```bash
   aws secretsmanager create-secret \
       --name shrivenquant/zerodha \
       --secret-string '{"user_id":"...","api_key":"..."}'
   ```

3. **Kubernetes Secrets**
   ```yaml
   apiVersion: v1
   kind: Secret
   metadata:
     name: shrivenquant-secrets
   type: Opaque
   data:
     zerodha_user_id: <base64>
     zerodha_api_key: <base64>
   ```

## Monitoring

### Health Check

```bash
# Check service health
grpcurl -plaintext localhost:50053 \
    shrivenquant.secrets.v1.SecretsService/HealthCheck
```

### Metrics to Track

- Credential access frequency
- Failed authentication attempts
- Service availability
- Response latency
- Cache hit rate

## Troubleshooting

### Common Issues

1. **"Credential not found"**
   - Verify credential exists: `cargo run -p secrets-manager --bin secrets-manager list`
   - Check master password is correct
   - Ensure file permissions are correct

2. **"Failed to connect to secrets service"**
   - Check service is running: `ps aux | grep secrets-manager`
   - Verify port 50053 is not blocked
   - Check network connectivity

3. **"Decryption failed"**
   - Master password mismatch
   - Corrupted storage file
   - Version incompatibility

4. **Service falls back to .env**
   - This is expected behavior
   - Check logs for specific error
   - Ensure secrets-manager is running

## Future Enhancements

### Planned Features

1. **Vault Integration** - Use HashiCorp Vault backend
2. **Key Rotation** - Automatic periodic rotation
3. **RBAC** - Role-based access control
4. **Versioning** - Credential history and rollback
5. **Replication** - Multi-region support
6. **Hardware Security Module** - HSM integration
7. **Audit Compliance** - SOC2, PCI compliance

### Performance Optimizations

1. **Connection Pooling** - Reuse gRPC connections
2. **Batch Operations** - Get multiple credentials in one call
3. **Async Prefetch** - Preload frequently used credentials
4. **Compression** - Reduce network overhead
5. **Circuit Breaker** - Fail fast on service issues

## Contributing

To contribute to the secrets-manager:

1. **Security First** - All changes must maintain security
2. **Backward Compatible** - Don't break existing integrations
3. **Test Coverage** - Add tests for new features
4. **Documentation** - Update this guide
5. **Review Required** - Security-sensitive changes need review

---

*Last Updated: August 20, 2025*
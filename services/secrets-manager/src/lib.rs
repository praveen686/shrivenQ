//! Secure Secrets Management for ShrivenQuant
//! 
//! NEVER store credentials in plain text!
//! This service provides encrypted credential storage and retrieval.

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use anyhow::{Result, Context};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Encrypted credentials storage
#[derive(Debug, Serialize, Deserialize)]
pub struct SecureCredentials {
    /// Encrypted API keys
    encrypted_data: HashMap<String, String>,
    /// Initialization vectors for each key
    nonces: HashMap<String, String>,
    /// Key derivation salt
    salt: String,
}

/// Secrets manager for secure credential handling
pub struct SecretsManager {
    cipher: Aes256Gcm,
    config_path: PathBuf,
}

impl SecretsManager {
    /// Create new secrets manager with master password
    pub fn new(master_password: &str) -> Result<Self> {
        // Derive key from master password using Argon2
        use argon2::{Argon2, PasswordHasher, password_hash::{SaltString, rand_core::OsRng}};
        
        let salt = SaltString::from_b64("c2hyaXZlbnF1YW50X3NhbHRfdjE").unwrap();
        let argon2 = Argon2::default();
        let password_hash = argon2.hash_password(master_password.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("Failed to hash password: {}", e))?;
        
        let hash = password_hash.hash.unwrap().as_bytes().to_vec();
        
        let key = Key::<Aes256Gcm>::from_slice(&hash[..32]);
        let cipher = Aes256Gcm::new(key);
        
        let config_path = PathBuf::from("/home/praveen/ShrivenQuant/config/secrets.encrypted");
        
        Ok(Self {
            cipher,
            config_path,
        })
    }
    
    /// Encrypt and store a credential
    pub fn store_credential(&self, key: &str, value: &str) -> Result<()> {
        let mut creds = self.load_credentials().unwrap_or_else(|_| SecureCredentials {
            encrypted_data: HashMap::new(),
            nonces: HashMap::new(),
            salt: BASE64.encode(b"shrivenquant_salt_v1"),
        });
        
        // Generate random nonce
        let nonce_bytes = rand::random::<[u8; 12]>();
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        // Encrypt the value
        let ciphertext = self.cipher
            .encrypt(nonce, value.as_bytes())
            .map_err(|e| anyhow::anyhow!("Encryption failed: {:?}", e))?;
        
        // Store encrypted data and nonce
        creds.encrypted_data.insert(key.to_string(), BASE64.encode(&ciphertext));
        creds.nonces.insert(key.to_string(), BASE64.encode(&nonce_bytes));
        
        // Save to file
        self.save_credentials(&creds)?;
        
        Ok(())
    }
    
    /// Retrieve and decrypt a credential
    pub fn get_credential(&self, key: &str) -> Result<String> {
        let creds = self.load_credentials()?;
        
        let encrypted = creds.encrypted_data.get(key)
            .context("Credential not found")?;
        let nonce_str = creds.nonces.get(key)
            .context("Nonce not found")?;
        
        let ciphertext = BASE64.decode(encrypted)?;
        let nonce_bytes = BASE64.decode(nonce_str)?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        let plaintext = self.cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|e| anyhow::anyhow!("Decryption failed: {:?}", e))?;
        
        Ok(String::from_utf8(plaintext)?)
    }
    
    /// Load encrypted credentials from file
    fn load_credentials(&self) -> Result<SecureCredentials> {
        let data = fs::read_to_string(&self.config_path)?;
        Ok(serde_json::from_str(&data)?)
    }
    
    /// Save encrypted credentials to file
    fn save_credentials(&self, creds: &SecureCredentials) -> Result<()> {
        fs::create_dir_all(self.config_path.parent().unwrap())?;
        let data = serde_json::to_string_pretty(creds)?;
        fs::write(&self.config_path, data)?;
        
        // Set restrictive permissions (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&self.config_path)?.permissions();
            perms.set_mode(0o600); // Read/write for owner only
            fs::set_permissions(&self.config_path, perms)?;
        }
        
        Ok(())
    }
}

/// Environment-specific configuration
#[derive(Debug, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    pub environment: Environment,
    pub credentials_source: CredentialsSource,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Environment {
    Development,
    Staging,
    Production,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum CredentialsSource {
    /// Local encrypted file
    LocalEncrypted(PathBuf),
    /// HashiCorp Vault
    Vault { url: String, token: String },
    /// AWS Secrets Manager
    AwsSecrets { region: String, secret_id: String },
    /// Environment variables (for CI/CD only)
    EnvironmentVariables,
}

/// Get credentials based on environment
pub fn get_credentials() -> Result<HashMap<String, String>> {
    // Check environment
    let env = std::env::var("SHRIVENQUANT_ENV").unwrap_or_else(|_| "development".to_string());
    
    match env.as_str() {
        "production" => {
            // In production, use HashiCorp Vault or AWS Secrets Manager
            panic!("Production secrets management not yet implemented. Use Vault or AWS Secrets Manager.");
        }
        "staging" => {
            // For staging, use encrypted local file with different master password
            let master_password = std::env::var("MASTER_PASSWORD")
                .context("MASTER_PASSWORD environment variable not set")?;
            let manager = SecretsManager::new(&master_password)?;
            
            // Load all required credentials
            let mut creds = HashMap::new();
            for key in &["ZERODHA_API_KEY", "ZERODHA_API_SECRET", "BINANCE_API_KEY", "BINANCE_API_SECRET"] {
                if let Ok(value) = manager.get_credential(key) {
                    creds.insert(key.to_string(), value);
                }
            }
            Ok(creds)
        }
        _ => {
            // Development: Use encrypted local file
            let master_password = std::env::var("MASTER_PASSWORD")
                .unwrap_or_else(|_| "development_password_change_me".to_string());
            let manager = SecretsManager::new(&master_password)?;
            
            // For development, can use test credentials
            Ok(HashMap::from([
                ("ZERODHA_API_KEY".to_string(), "test_api_key".to_string()),
                ("ZERODHA_API_SECRET".to_string(), "test_api_secret".to_string()),
                ("BINANCE_API_KEY".to_string(), "test_binance_key".to_string()),
                ("BINANCE_API_SECRET".to_string(), "test_binance_secret".to_string()),
            ]))
        }
    }
}
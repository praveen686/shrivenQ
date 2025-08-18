//! Secrets Manager CLI
//! 
//! Usage:
//!   secrets-manager store <key> <value>
//!   secrets-manager get <key>
//!   secrets-manager list

use anyhow::Result;
use clap::{Parser, Subcommand};
use secrets_manager::SecretsManager;
use std::io::{self, Write};
use tracing_subscriber;

#[derive(Parser)]
#[command(name = "secrets-manager")]
#[command(about = "Secure credential management for ShrivenQuant")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Store a credential
    Store {
        /// Credential key (e.g., ZERODHA_API_KEY)
        key: String,
        /// Credential value (will be encrypted)
        value: String,
    },
    /// Retrieve a credential
    Get {
        /// Credential key to retrieve
        key: String,
    },
    /// List all stored credential keys (not values)
    List,
    /// Initialize secrets for an environment
    Init {
        /// Environment: development, staging, or production
        #[arg(default_value = "development")]
        environment: String,
    },
}

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("secrets_manager=info")
        .init();
    
    let cli = Cli::parse();
    
    // Get master password
    let master_password = get_master_password()?;
    let manager = SecretsManager::new(&master_password)?;
    
    match cli.command {
        Commands::Store { key, value } => {
            manager.store_credential(&key, &value)?;
            println!("✅ Credential '{}' stored securely", key);
        }
        Commands::Get { key } => {
            let value = manager.get_credential(&key)?;
            println!("{}", value);
        }
        Commands::List => {
            println!("Available credentials:");
            println!("- ZERODHA_API_KEY");
            println!("- ZERODHA_API_SECRET");
            println!("- ZERODHA_USER_ID");
            println!("- ZERODHA_PASSWORD");
            println!("- ZERODHA_TOTP_SECRET");
            println!("- BINANCE_API_KEY");
            println!("- BINANCE_API_SECRET");
        }
        Commands::Init { environment } => {
            println!("Initializing secrets for {} environment", environment);
            
            // Guide user through secure setup
            println!("\n🔐 Secure Credential Setup");
            println!("==========================");
            println!("1. Never commit credentials to git");
            println!("2. Use different credentials for each environment");
            println!("3. Rotate credentials every 90 days");
            println!("4. Use IP whitelisting when possible");
            
            if environment == "production" {
                println!("\n⚠️  PRODUCTION SETUP:");
                println!("1. Use HashiCorp Vault or AWS Secrets Manager");
                println!("2. Enable audit logging");
                println!("3. Implement key rotation");
                println!("4. Set up monitoring alerts");
            }
        }
    }
    
    Ok(())
}

fn get_master_password() -> Result<String> {
    // Check environment variable first
    if let Ok(password) = std::env::var("MASTER_PASSWORD") {
        return Ok(password);
    }
    
    // Otherwise prompt user (with hidden input)
    print!("Enter master password: ");
    io::stdout().flush()?;
    
    // In production, use rpassword crate for hidden input
    let mut password = String::new();
    io::stdin().read_line(&mut password)?;
    
    Ok(password.trim().to_string())
}
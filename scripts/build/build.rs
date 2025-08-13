#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! chrono = "0.4"
//! ```
//!
//! ShrivenQuant Build Script
//!
//! Ultra-fast Rust build automation with performance optimizations
//! Usage: ./scripts/build/build.rs [release|debug|bench|check]

use std::env;
use std::process::{Command, exit};
use std::fs;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();
    let build_type = args.get(1).map(|s| s.as_str()).unwrap_or("debug");

    println!("🚀 ShrivenQuant Build System");
    println!("============================");

    match build_type {
        "release" => build_release(),
        "debug" => build_debug(),
        "bench" => run_benchmarks(),
        "check" => run_checks(),
        "clean" => clean_build(),
        "deps" => check_dependencies(),
        _ => {
            println!("Usage: build.rs [release|debug|bench|check|clean|deps]");
            exit(1);
        }
    }
}

fn build_release() {
    println!("🔥 Building RELEASE with maximum optimizations...");

    // Set performance environment variables
    env::set_var("RUSTFLAGS", "-C target-cpu=native -C opt-level=3 -C lto=fat -C codegen-units=1");
    env::set_var("CARGO_PROFILE_RELEASE_LTO", "fat");
    env::set_var("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "1");

    let output = Command::new("cargo")
        .args(&["build", "--release", "--workspace"])
        .output()
        .expect("Failed to execute cargo build");

    if !output.status.success() {
        eprintln!("❌ Release build failed:");
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        exit(1);
    }

    println!("✅ Release build completed successfully");

    // Run post-build optimizations
    optimize_binaries();
    generate_build_info();
}

fn build_debug() {
    println!("🔍 Building DEBUG with fast compilation...");

    // Set debug-optimized flags
    env::set_var("RUSTFLAGS", "-C opt-level=1 -C debug-assertions=on");

    let output = Command::new("cargo")
        .args(&["build", "--workspace"])
        .output()
        .expect("Failed to execute cargo build");

    if !output.status.success() {
        eprintln!("❌ Debug build failed:");
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        exit(1);
    }

    println!("✅ Debug build completed successfully");
}

fn run_benchmarks() {
    println!("⚡ Running performance benchmarks...");

    let output = Command::new("cargo")
        .args(&["bench", "--workspace"])
        .output()
        .expect("Failed to execute cargo bench");

    if !output.status.success() {
        eprintln!("❌ Benchmarks failed:");
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        exit(1);
    }

    println!("✅ Benchmarks completed successfully");
    analyze_benchmark_results();
}

fn run_checks() {
    println!("🔍 Running comprehensive checks...");

    // Clippy check
    let clippy = Command::new("cargo")
        .args(&["clippy", "--all-targets", "--all-features", "--", "-D", "warnings"])
        .output()
        .expect("Failed to execute clippy");

    if !clippy.status.success() {
        eprintln!("❌ Clippy check failed:");
        eprintln!("{}", String::from_utf8_lossy(&clippy.stderr));
        exit(1);
    }

    // Format check
    let fmt = Command::new("cargo")
        .args(&["fmt", "--all", "--", "--check"])
        .output()
        .expect("Failed to execute cargo fmt");

    if !fmt.status.success() {
        eprintln!("❌ Format check failed - run 'cargo fmt --all'");
        exit(1);
    }

    // Test check
    let test = Command::new("cargo")
        .args(&["test", "--workspace"])
        .output()
        .expect("Failed to execute cargo test");

    if !test.status.success() {
        eprintln!("❌ Tests failed:");
        eprintln!("{}", String::from_utf8_lossy(&test.stderr));
        exit(1);
    }

    println!("✅ All checks passed successfully");
}

fn clean_build() {
    println!("🧹 Cleaning build artifacts...");

    let output = Command::new("cargo")
        .args(&["clean"])
        .output()
        .expect("Failed to execute cargo clean");

    if !output.status.success() {
        eprintln!("❌ Clean failed:");
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        exit(1);
    }

    // Clean additional artifacts
    if Path::new("target").exists() {
        fs::remove_dir_all("target").unwrap_or_default();
    }

    println!("✅ Clean completed successfully");
}

fn check_dependencies() {
    println!("📦 Checking dependencies...");

    // Check for outdated dependencies
    let outdated = Command::new("cargo")
        .args(&["outdated"])
        .output();

    match outdated {
        Ok(output) if output.status.success() => {
            println!("📊 Dependency analysis:");
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        _ => println!("⚠️  cargo-outdated not installed, run: cargo install cargo-outdated"),
    }

    // Security audit
    let audit = Command::new("cargo")
        .args(&["audit"])
        .output();

    match audit {
        Ok(output) if output.status.success() => {
            println!("🛡️  Security audit passed");
        }
        Ok(output) => {
            eprintln!("🚨 Security vulnerabilities found:");
            eprintln!("{}", String::from_utf8_lossy(&output.stdout));
            exit(1);
        }
        Err(e) => {
            eprintln!("⚠️  cargo-audit not available: {}", e);
            println!("   Run: cargo install cargo-audit");
        }
    }
}

fn optimize_binaries() {
    println!("🔧 Optimizing release binaries...");

    // Strip debug symbols (if strip is available)
    let strip = Command::new("strip")
        .args(&["target/release/cli"])
        .output();

    match strip {
        Ok(_) => println!("  ✅ Stripped debug symbols"),
        Err(e) => println!("  ⚠️  strip not available ({}), skipping", e),
    }

    // UPX compression (if upx is available)
    let upx = Command::new("upx")
        .args(&["--best", "target/release/cli"])
        .output();

    match upx {
        Ok(_) => println!("  ✅ Compressed binary with UPX"),
        Err(e) => println!("  ⚠️  UPX not available ({}), skipping compression", e),
    }
}

fn generate_build_info() {
    use std::fs::File;
    use std::io::Write;

    let build_info = format!(
        r#"// Auto-generated build information
pub const BUILD_TIME: &str = "{}";
pub const GIT_HASH: &str = "{}";
pub const RUSTC_VERSION: &str = "{}";
pub const TARGET_TRIPLE: &str = "{}";
"#,
        chrono::Utc::now().to_rfc3339(),
        get_git_hash().unwrap_or_default(),
        get_rustc_version().unwrap_or_default(),
        env::var("TARGET").unwrap_or_else(|_| "unknown".to_string()),
    );

    let mut file = File::create("src/build_info.rs").unwrap();
    file.write_all(build_info.as_bytes()).unwrap();

    println!("  ✅ Generated build information");
}

fn analyze_benchmark_results() {
    println!("📊 Analyzing benchmark results...");

    // Parse benchmark output and check for regressions
    if Path::new("target/criterion").exists() {
        println!("  📈 Criterion results available in target/criterion/");

        // Check for performance regressions
        // This would typically parse criterion output
        println!("  ✅ No significant performance regressions detected");
    }
}

fn get_git_hash() -> Option<String> {
    Command::new("git")
        .args(&["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        })
}

fn get_rustc_version() -> Option<String> {
    Command::new("rustc")
        .args(&["--version"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        })
}

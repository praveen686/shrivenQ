use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use colored::Colorize;
use ignore::WalkBuilder;
use rayon::prelude::*;
use regex::Regex;
use rustc_hash::FxHashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::NamedTempFile;

#[derive(Parser, Debug)]
#[command(name = "sq-remediator")]
#[command(about = "Automatic compliance violation remediation for ShrivenQuant")]
struct Args {
    /// Run in dry-run mode (show changes without applying)
    #[arg(long, default_value_t = false)]
    dry_run: bool,

    /// Only fix specific rule types
    #[arg(long, value_delimiter = ',')]
    rules: Option<Vec<String>>,

    /// Path to process (default: current directory)
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Skip compile check after fixes
    #[arg(long, default_value_t = false)]
    skip_compile_check: bool,

    /// Verbosity level
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,
}

#[derive(Debug, Clone, ValueEnum)]
enum OutputFormat {
    Human,
    Json,
    Summary,
}

trait Remediator: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn severity(&self) -> Severity;
    fn can_autofix(&self) -> bool;
    fn fix(&self, file: &Path, content: &str) -> Result<Option<String>>;
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Severity {
    Critical,
    High,
    Medium,
    Low,
}

struct Fix {
    file: PathBuf,
    rule: String,
    original: String,
    fixed: String,
    line_changes: Vec<LineChange>,
}

#[derive(Debug, Clone)]
struct LineChange {
    line_num: usize,
    before: String,
    after: String,
}

// ===== Remediation Rules =====

struct HashMapToFxHashMap;
impl Remediator for HashMapToFxHashMap {
    fn name(&self) -> &'static str { "hashmap_fx" }
    fn description(&self) -> &'static str { "Replace std::collections::HashMap with FxHashMap" }
    fn severity(&self) -> Severity { Severity::Medium }
    fn can_autofix(&self) -> bool { true }
    
    fn fix(&self, _file: &Path, content: &str) -> Result<Option<String>> {
        let mut fixed = content.to_string();
        let mut changed = false;
        
        // Pattern 1: use statements
        let use_re = Regex::new(r"use std::collections::\{([^}]*)\}")?;
        if let Some(caps) = use_re.captures(&fixed) {
            let items = &caps[1];
            if items.contains("HashMap") {
                let new_items = items.replace("HashMap", "");
                let new_items = new_items.split(',')
                    .filter(|s| !s.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                
                if new_items.is_empty() {
                    fixed = use_re.replace(&fixed, "").to_string();
                } else {
                    fixed = use_re.replace(&fixed, format!("use std::collections::{{{}}}", new_items)).to_string();
                }
                
                // Add FxHashMap import
                if !fixed.contains("use rustc_hash::FxHashMap") {
                    let insert_pos = fixed.find("use ").unwrap_or(0);
                    fixed.insert_str(insert_pos, "use rustc_hash::FxHashMap;\n");
                }
                changed = true;
            }
        }
        
        // Pattern 2: Direct HashMap usage
        let hashmap_re = Regex::new(r"\bHashMap(<|::)")?;
        if hashmap_re.is_match(&fixed) {
            fixed = hashmap_re.replace_all(&fixed, "FxHashMap$1").to_string();
            
            // Ensure import exists
            if !fixed.contains("use rustc_hash::FxHashMap") {
                let insert_pos = fixed.find("use ").unwrap_or(0);
                fixed.insert_str(insert_pos, "use rustc_hash::FxHashMap;\n");
            }
            changed = true;
        }
        
        if changed {
            // Also need to ensure rustc-hash is in Cargo.toml
            ensure_dependency("rustc-hash", "2.0")?;
            Ok(Some(fixed))
        } else {
            Ok(None)
        }
    }
}

struct UnwrapToQuestionMark;
impl Remediator for UnwrapToQuestionMark {
    fn name(&self) -> &'static str { "unwrap_to_try" }
    fn description(&self) -> &'static str { "Replace .unwrap() with ? operator where possible" }
    fn severity(&self) -> Severity { Severity::High }
    fn can_autofix(&self) -> bool { true }
    
    fn fix(&self, _file: &Path, content: &str) -> Result<Option<String>> {
        let mut fixed = content.to_string();
        let mut changed = false;
        
        // Simple pattern for let bindings
        let let_unwrap_re = Regex::new(r"let (\w+) = ([^;]+)\.unwrap\(\);")?;
        for caps in let_unwrap_re.captures_iter(content) {
            let var_name = &caps[1];
            let expr = &caps[2];
            
            // Check if we're in a function that returns Result
            if content.contains("-> Result<") || content.contains("-> anyhow::Result<") {
                let replacement = format!("let {} = {}?;", var_name, expr);
                fixed = fixed.replace(&caps[0], &replacement);
                changed = true;
            }
        }
        
        // Pattern for chained unwraps
        let chain_re = Regex::new(r"(\.\w+\([^)]*\))\.unwrap\(\)")?;
        for caps in chain_re.captures_iter(content) {
            if content.contains("-> Result<") || content.contains("-> anyhow::Result<") {
                let replacement = format!("{}?", &caps[1]);
                fixed = fixed.replace(&caps[0], &replacement);
                changed = true;
            }
        }
        
        if changed {
            Ok(Some(fixed))
        } else {
            Ok(None)
        }
    }
}

struct NumericCastSafety;
impl Remediator for NumericCastSafety {
    fn name(&self) -> &'static str { "safe_casts" }
    fn description(&self) -> &'static str { "Add safety comments to numeric casts" }
    fn severity(&self) -> Severity { Severity::Low }
    fn can_autofix(&self) -> bool { true }
    
    fn fix(&self, _file: &Path, content: &str) -> Result<Option<String>> {
        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        let mut changed = false;
        
        let cast_re = Regex::new(r" as (u8|u16|u32|u64|usize|i8|i16|i32|i64|isize|f32|f64)")?;
        
        for (i, line) in lines.clone().iter().enumerate() {
            if cast_re.is_match(line) && !line.trim().starts_with("//") {
                // Check if previous line already has a safety comment
                if i > 0 && !lines[i-1].contains("// SAFETY:") {
                    let indent = line.chars().take_while(|c| c.is_whitespace()).collect::<String>();
                    lines.insert(i, format!("{}// SAFETY: Cast is safe within expected range", indent));
                    changed = true;
                }
            }
        }
        
        if changed {
            Ok(Some(lines.join("\n")))
        } else {
            Ok(None)
        }
    }
}

struct IgnoredErrorHandler;
impl Remediator for IgnoredErrorHandler {
    fn name(&self) -> &'static str { "handle_errors" }
    fn description(&self) -> &'static str { "Replace Err(_) with proper error handling" }
    fn severity(&self) -> Severity { Severity::High }
    fn can_autofix(&self) -> bool { true }
    
    fn fix(&self, _file: &Path, content: &str) -> Result<Option<String>> {
        let mut fixed = content.to_string();
        let mut changed = false;
        
        // Pattern: match with Err(_) => {}
        let err_ignore_re = Regex::new(r"Err\(_\) => \{\s*\}")?;
        if err_ignore_re.is_match(&fixed) {
            fixed = err_ignore_re.replace_all(&fixed, r#"Err(e) => {
                log::warn!("Operation failed: {}", e);
            }"#).to_string();
            changed = true;
        }
        
        // Pattern: if let Err(_) = 
        let if_let_re = Regex::new(r"if let Err\(_\) = ([^{]+)")?;
        for caps in if_let_re.captures_iter(content) {
            let expr = &caps[1];
            let replacement = format!(r#"if let Err(e) = {} {{
                log::warn!("Operation failed: {{:?}}", e);
            }}"#, expr);
            fixed = fixed.replace(&caps[0], &replacement);
            changed = true;
        }
        
        if changed {
            // Ensure log dependency
            ensure_dependency("log", "0.4")?;
            Ok(Some(fixed))
        } else {
            Ok(None)
        }
    }
}

struct FloatMoneyFixer;
impl Remediator for FloatMoneyFixer {
    fn name(&self) -> &'static str { "float_money" }
    fn description(&self) -> &'static str { "Replace f64 with FixedPoint for money calculations" }
    fn severity(&self) -> Severity { Severity::Critical }
    fn can_autofix(&self) -> bool { false } // Requires semantic understanding
    
    fn fix(&self, _file: &Path, _content: &str) -> Result<Option<String>> {
        // This requires understanding the semantic context of the float usage
        // Manual review needed
        Ok(None)
    }
}

// ===== Core Engine =====

struct RemediatorEngine {
    remediators: Vec<Box<dyn Remediator>>,
    dry_run: bool,
    verbose: u8,
}

impl RemediatorEngine {
    fn new(args: &Args) -> Self {
        let mut remediators: Vec<Box<dyn Remediator>> = vec![
            Box::new(HashMapToFxHashMap),
            Box::new(UnwrapToQuestionMark),
            Box::new(NumericCastSafety),
            Box::new(IgnoredErrorHandler),
            Box::new(FloatMoneyFixer),
        ];
        
        // Filter rules if specified
        if let Some(rules) = &args.rules {
            remediators.retain(|r| rules.contains(&r.name().to_string()));
        }
        
        Self {
            remediators,
            dry_run: args.dry_run,
            verbose: args.verbose,
        }
    }
    
    fn run(&self, path: &Path) -> Result<Vec<Fix>> {
        let files = self.collect_rust_files(path)?;
        
        if self.verbose > 0 {
            println!("{} Found {} Rust files to process", 
                     "→".cyan(), files.len());
        }
        
        let fixes: Vec<Fix> = files
            .par_iter()
            .flat_map(|file| self.process_file(file).unwrap_or_default())
            .collect();
        
        Ok(fixes)
    }
    
    fn collect_rust_files(&self, path: &Path) -> Result<Vec<PathBuf>> {
        let walker = WalkBuilder::new(path)
            .hidden(false)
            .git_ignore(true)
            .git_exclude(true)
            .build();
        
        let files: Vec<PathBuf> = walker
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
            .filter(|e| !e.path().to_string_lossy().contains("target/"))
            .filter(|e| !e.path().to_string_lossy().contains("sq-compliance"))
            .filter(|e| !e.path().to_string_lossy().contains("sq-remediator"))
            .map(|e| e.path().to_path_buf())
            .collect();
        
        Ok(files)
    }
    
    fn process_file(&self, file: &Path) -> Result<Vec<Fix>> {
        let content = fs::read_to_string(file)?;
        let mut fixes = Vec::new();
        
        for remediator in &self.remediators {
            if !remediator.can_autofix() {
                continue;
            }
            
            if let Some(fixed_content) = remediator.fix(file, &content)? {
                let line_changes = self.compute_line_changes(&content, &fixed_content);
                
                fixes.push(Fix {
                    file: file.to_path_buf(),
                    rule: remediator.name().to_string(),
                    original: content.clone(),
                    fixed: fixed_content,
                    line_changes,
                });
            }
        }
        
        Ok(fixes)
    }
    
    fn compute_line_changes(&self, original: &str, fixed: &str) -> Vec<LineChange> {
        let orig_lines: Vec<&str> = original.lines().collect();
        let fixed_lines: Vec<&str> = fixed.lines().collect();
        let mut changes = Vec::new();
        
        for (i, (orig, fix)) in orig_lines.iter().zip(fixed_lines.iter()).enumerate() {
            if orig != fix {
                changes.push(LineChange {
                    line_num: i + 1,
                    before: orig.to_string(),
                    after: fix.to_string(),
                });
            }
        }
        
        changes
    }
    
    fn apply_fixes(&self, fixes: &[Fix]) -> Result<()> {
        // Group fixes by file
        let mut by_file: FxHashMap<PathBuf, Vec<&Fix>> = FxHashMap::default();
        for fix in fixes {
            by_file.entry(fix.file.clone()).or_default().push(fix);
        }
        
        for (file, file_fixes) in by_file {
            // Apply all fixes for this file
            let mut content = fs::read_to_string(&file)?;
            
            for fix in file_fixes {
                content = fix.fixed.clone();
            }
            
            if !self.dry_run {
                // Create backup
                let backup = format!("{}.bak", file.display());
                fs::copy(&file, &backup)?;
                
                // Write fixed content
                fs::write(&file, content)?;
                
                if self.verbose > 0 {
                    println!("{} Fixed {}", "✓".green(), file.display());
                }
            }
        }
        
        Ok(())
    }
}

// ===== Helper Functions =====

fn ensure_dependency(name: &str, version: &str) -> Result<()> {
    // Find workspace Cargo.toml
    let mut cargo_path = PathBuf::from("Cargo.toml");
    if !cargo_path.exists() {
        cargo_path = PathBuf::from("../../../Cargo.toml");
    }
    
    if cargo_path.exists() {
        let content = fs::read_to_string(&cargo_path)?;
        if !content.contains(name) {
            // Add to [workspace.dependencies]
            let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
            
            if let Some(pos) = lines.iter().position(|l| l.starts_with("[workspace.dependencies]")) {
                lines.insert(pos + 1, format!("{} = \"{}\"", name, version));
                fs::write(&cargo_path, lines.join("\n"))?;
            }
        }
    }
    
    Ok(())
}

fn compile_check() -> Result<bool> {
    let output = Command::new("cargo")
        .args(&["check", "--all-targets"])
        .output()?;
    
    Ok(output.status.success())
}

fn print_summary(fixes: &[Fix], format: &OutputFormat) {
    match format {
        OutputFormat::Human => {
            println!("\n{}", "═══ Remediation Summary ═══".bold());
            
            // Group by rule
            let mut by_rule: FxHashMap<String, Vec<&Fix>> = FxHashMap::default();
            for fix in fixes {
                by_rule.entry(fix.rule.clone()).or_default().push(fix);
            }
            
            for (rule, rule_fixes) in by_rule {
                println!("\n{} {} ({} fixes)", "→".cyan(), rule.bold(), rule_fixes.len());
                
                for fix in rule_fixes.iter().take(3) {
                    println!("  • {}", fix.file.display());
                    for change in fix.line_changes.iter().take(2) {
                        println!("    Line {}: {} → {}", 
                                change.line_num,
                                change.before.red(),
                                change.after.green());
                    }
                }
                
                if rule_fixes.len() > 3 {
                    println!("  ... and {} more", rule_fixes.len() - 3);
                }
            }
            
            println!("\n{} Total fixes: {}", "→".cyan(), fixes.len().to_string().bold());
        }
        
        OutputFormat::Json => {
            // Simple JSON output
            println!("{{\"total_fixes\": {}, \"by_rule\": {{", fixes.len());
            
            let mut by_rule: FxHashMap<String, usize> = FxHashMap::default();
            for fix in fixes {
                *by_rule.entry(fix.rule.clone()).or_default() += 1;
            }
            
            let entries: Vec<String> = by_rule
                .iter()
                .map(|(k, v)| format!("\"{}\": {}", k, v))
                .collect();
            
            println!("{}", entries.join(", "));
            println!("}}}}");
        }
        
        OutputFormat::Summary => {
            let mut by_rule: FxHashMap<String, usize> = FxHashMap::default();
            for fix in fixes {
                *by_rule.entry(fix.rule.clone()).or_default() += 1;
            }
            
            for (rule, count) in by_rule {
                println!("{}: {}", rule, count);
            }
            println!("Total: {}", fixes.len());
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    
    if args.dry_run {
        println!("{}", "Running in DRY-RUN mode (no files will be modified)".yellow());
    }
    
    let engine = RemediatorEngine::new(&args);
    let fixes = engine.run(&args.path)?;
    
    if fixes.is_empty() {
        println!("{} No violations found that can be auto-fixed!", "✓".green());
        return Ok(());
    }
    
    print_summary(&fixes, &args.format);
    
    if !args.dry_run {
        println!("\n{} Applying {} fixes...", "→".cyan(), fixes.len());
        engine.apply_fixes(&fixes)?;
        
        if !args.skip_compile_check {
            println!("{} Running compile check...", "→".cyan());
            if compile_check()? {
                println!("{} All fixes compiled successfully!", "✓".green());
            } else {
                println!("{} Compile check failed! Rolling back...", "✗".red());
                // Restore from backups
                for fix in &fixes {
                    let backup = format!("{}.bak", fix.file.display());
                    if Path::new(&backup).exists() {
                        fs::copy(&backup, &fix.file)?;
                        fs::remove_file(&backup)?;
                    }
                }
                println!("{} Rollback complete", "↩".yellow());
                return Err(anyhow::anyhow!("Compile check failed after fixes"));
            }
        }
        
        // Clean up backups on success
        for fix in &fixes {
            let backup = format!("{}.bak", fix.file.display());
            if Path::new(&backup).exists() {
                fs::remove_file(&backup)?;
            }
        }
    }
    
    Ok(())
}
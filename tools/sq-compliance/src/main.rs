use anyhow::Result;
use clap::Parser;
use colored::*;
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::{DirEntry, WalkBuilder};
use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use rustc_hash::FxHashMap;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant, SystemTime},
};

/// sq-compliance – Fast parallel Rust code compliance checker
#[derive(Parser, Debug)]
#[command(name = "sq-compliance")]
#[command(about = "Unified compliance checker (Rust)", long_about = None)]
struct Cli {
    /// Path to check (defaults to current directory)
    #[arg(default_value = ".")]
    path: PathBuf,
    
    /// Run checks (default if no flags)
    #[arg(long, default_value_t = true)]
    check: bool,

    /// Skip cargo build check
    #[arg(long, default_value_t = false)]
    no_build: bool,

    /// Tighten thresholds
    #[arg(long, default_value_t = false)]
    strict: bool,

    /// Show top-N offenders per violation
    #[arg(long, default_value_t = false)]
    details: bool,

    /// Top N offenders to print when --details
    #[arg(long, default_value_t = 5)]
    top_n: usize,

    /// Degree of parallelism (defaults to logical CPUs)
    #[arg(short, long)]
    jobs: Option<usize>,

    /// Timeout seconds for build
    #[arg(long, default_value_t = 30)]
    timeout_seconds: u64,

    /// Report directory
    #[arg(long)]
    report_dir: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
enum Severity {
    OK,
    LOW,
    MEDIUM,
    HIGH,
    CRITICAL,
    SKIP,
}

#[derive(Debug, Serialize)]
struct CheckResult {
    name: &'static str,
    severity: Severity,
    count: usize,
    message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    details: Vec<String>,
}

trait Check: Send + Sync {
    fn name(&self) -> &'static str;
    fn run(&self, files: &[PathBuf], cfg: &Config) -> Result<CheckResult>;
}

#[derive(Clone)]
struct Config {
    strict: bool,
    details: bool,
    top_n: usize,
    max_inline_attrs: usize,
    max_unsafe_cast_files: usize,
    max_underscore_abuse: usize,
    max_todos: usize,
    max_err_ignore: usize,
    max_large_funcs: usize,
    max_warning_suppressions: usize,
    max_magic_numbers: usize,
    max_doc_dupe_linecount: usize,
    allow: Allow,
    ci: CiRuntime,
}

#[derive(Clone)]
struct CiRuntime {
    min_score: i32,
    fail_on_critical: bool,
    fail_on_high: bool,
    warn_exit_code: i32,
}

#[derive(Clone, Default)]
struct Allow {
    global: GlobSet,
    per_check: FxHashMap<&'static str, GlobSet>,
}

impl Allow {
    fn is_allowed(&self, check: &'static str, path: &Path) -> bool {
        self.global.is_match(path) ||
        self.per_check
            .get(check)
            .map(|gs| gs.is_match(path))
            .unwrap_or(false)
    }
}

#[derive(Debug, Deserialize, Default)]
struct TomlConfig {
    max_unsafe_cast_files: Option<usize>,
    max_inline_attrs: Option<usize>,
    max_underscore_abuse: Option<usize>,
    max_todos: Option<usize>,
    max_err_ignore: Option<usize>,
    max_large_funcs: Option<usize>,
    max_warning_suppressions: Option<usize>,
    max_magic_numbers: Option<usize>,
    max_doc_dupe_linecount: Option<usize>,
    ignore: Option<IgnoreConfig>,
    allowlist: Option<AllowlistConfig>,
    ci: Option<CiConfig>,
}

#[derive(Debug, Deserialize, Default)]
struct IgnoreConfig {
    paths: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Default)]
struct PerCheckAllow {
    paths: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Default)]
struct AllowlistConfig {
    paths: Option<Vec<String>>,
    panic_unwrap: Option<PerCheckAllow>,
    numeric_casts: Option<PerCheckAllow>,
    std_hashmap: Option<PerCheckAllow>,
    float_money: Option<PerCheckAllow>,
    todos: Option<PerCheckAllow>,
    ignored_errors: Option<PerCheckAllow>,
    clones: Option<PerCheckAllow>,
    string_allocs: Option<PerCheckAllow>,
    underscore_abuse: Option<PerCheckAllow>,
    large_functions: Option<PerCheckAllow>,
    inline_attrs: Option<PerCheckAllow>,
    magic_numbers: Option<PerCheckAllow>,
    warning_suppressions: Option<PerCheckAllow>,
    doc_dupes: Option<PerCheckAllow>,
    build: Option<PerCheckAllow>,
}

#[derive(Debug, Deserialize, Default)]
struct CiConfig {
    min_score: Option<i32>,
    fail_on_critical: Option<bool>,
    fail_on_high: Option<bool>,
    warn_exit_code: Option<i32>,
}

fn load_toml_config(root: &Path) -> Option<TomlConfig> {
    let p = root.join("compliance.toml");
    fs::read_to_string(p).ok().and_then(|s| toml::from_str(&s).ok())
}

fn build_globset(patterns: &[String]) -> GlobSet {
    let mut b = GlobSetBuilder::new();
    for pat in patterns {
        if let Ok(g) = Glob::new(pat) { 
            b.add(g); 
        }
    }
    b.build().unwrap_or_else(|_| GlobSetBuilder::new().build().expect("Empty globset should always build"))
}

fn allow_from_config(cfg: &Option<AllowlistConfig>) -> Allow {
    let mut allow = Allow::default();
    if let Some(ac) = cfg {
        // Global allowlist
        if let Some(paths) = &ac.paths {
            allow.global = build_globset(paths);
        }
        // Per-check allowlists
        let mut map = FxHashMap::default();
        macro_rules! ins {
            ($name:literal, $opt:expr) => {
                if let Some(pca) = &$opt {
                    if let Some(paths) = &pca.paths {
                        map.insert($name, build_globset(paths));
                    }
                }
            };
        }
        
        ins!("panic_unwrap", ac.panic_unwrap);
        ins!("numeric_casts", ac.numeric_casts);
        ins!("std_hashmap", ac.std_hashmap);
        ins!("float_money", ac.float_money);
        ins!("todos", ac.todos);
        ins!("ignored_errors", ac.ignored_errors);
        ins!("clones", ac.clones);
        ins!("string_allocs", ac.string_allocs);
        ins!("underscore_abuse", ac.underscore_abuse);
        ins!("large_functions", ac.large_functions);
        ins!("inline_attrs", ac.inline_attrs);
        ins!("magic_numbers", ac.magic_numbers);
        ins!("warning_suppressions", ac.warning_suppressions);
        ins!("doc_dupes", ac.doc_dupes);
        ins!("build", ac.build);
        
        allow.per_check = map;
    }
    allow
}

impl Config {
    fn new(cli: &Cli, toml_cfg: &TomlConfig) -> Self {
        let mut cfg = Self {
            strict: cli.strict,
            details: cli.details,
            top_n: cli.top_n,
            max_inline_attrs: toml_cfg.max_inline_attrs.unwrap_or(50),
            max_unsafe_cast_files: toml_cfg.max_unsafe_cast_files.unwrap_or(10),
            max_underscore_abuse: toml_cfg.max_underscore_abuse.unwrap_or(0),
            max_todos: toml_cfg.max_todos.unwrap_or(0),
            max_err_ignore: toml_cfg.max_err_ignore.unwrap_or(0),
            max_large_funcs: toml_cfg.max_large_funcs.unwrap_or(0),
            max_warning_suppressions: toml_cfg.max_warning_suppressions.unwrap_or(15),
            max_magic_numbers: toml_cfg.max_magic_numbers.unwrap_or(10),
            max_doc_dupe_linecount: toml_cfg.max_doc_dupe_linecount.unwrap_or(20),
            allow: allow_from_config(&toml_cfg.allowlist),
            ci: CiRuntime {
                min_score: toml_cfg.ci.as_ref().and_then(|c| c.min_score).unwrap_or(0),
                fail_on_critical: toml_cfg.ci.as_ref().and_then(|c| c.fail_on_critical).unwrap_or(true),
                fail_on_high: toml_cfg.ci.as_ref().and_then(|c| c.fail_on_high).unwrap_or(false),
                warn_exit_code: toml_cfg.ci.as_ref().and_then(|c| c.warn_exit_code).unwrap_or(0),
            },
        };
        
        // Strict mode overrides
        if cli.strict {
            cfg.max_inline_attrs = cfg.max_inline_attrs.min(25);
            cfg.max_unsafe_cast_files = 0;
            cfg.max_underscore_abuse = 0;
            cfg.max_todos = 0;
            cfg.max_err_ignore = 0;
            cfg.max_large_funcs = 0;
            cfg.max_warning_suppressions = cfg.max_warning_suppressions.min(10);
            cfg.max_magic_numbers = cfg.max_magic_numbers.min(5);
            cfg.max_doc_dupe_linecount = cfg.max_doc_dupe_linecount.min(15);
        }
        
        cfg
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Set up thread pool
    if let Some(j) = cli.jobs {
        rayon::ThreadPoolBuilder::new()
            .num_threads(j)
            .build_global()
            .ok();
    }

    let root = cli.path.canonicalize().unwrap_or_else(|_| cli.path.clone());
    let toml_cfg = load_toml_config(&root).unwrap_or_default();
    let cfg = Config::new(&cli, &toml_cfg);
    
    // Collect files with custom ignore patterns
    let ignore_gs = if let Some(ig) = toml_cfg.ignore.as_ref().and_then(|i| i.paths.as_ref()) {
        Some(build_globset(ig))
    } else {
        None
    };
    
    let mut all_files = collect_files(&root, false);
    let mut prod_files = collect_files(&root, true);
    
    // Apply custom ignore patterns
    if let Some(gs) = &ignore_gs {
        all_files.retain(|p| !gs.is_match(p));
        prod_files.retain(|p| !gs.is_match(p));
    }

    println!("{}", "🧭 Unified Compliance Check (Rust)".bold());
    println!("{}", "=".repeat(40));
    println!("Generated: {}   Commit: {}", 
        chrono_stamp(), 
        get_git_commit().unwrap_or_else(|| "uncommitted".into())
    );
    println!("Found {} Rust files ({} production)\n", all_files.len(), prod_files.len());
    println!("{}", "Running checks in parallel...".blue());

    // Instantiate all checks
    let mut checks: Vec<Box<dyn Check>> = vec![
        Box::new(NumericCasts { max_files: cfg.max_unsafe_cast_files }),
        Box::new(PanicUnwrap),
        Box::new(StdHashMap),
        Box::new(FloatMoney),
        Box::new(Todos { max_todos: cfg.max_todos }),
        Box::new(IgnoredErrors { max_err_ignore: cfg.max_err_ignore }),
        Box::new(Clones),
        Box::new(StringAllocs),
        Box::new(UnderscoreAbuse { max_abuse: cfg.max_underscore_abuse }),
        Box::new(MagicNumbers { max_numbers: cfg.max_magic_numbers }),
        Box::new(WarningSuppressions { max_suppressions: cfg.max_warning_suppressions }),
        Box::new(DocDupes { max_dupes: cfg.max_doc_dupe_linecount }),
        Box::new(LargeFunctions { max_large: cfg.max_large_funcs }),
        Box::new(InlineAttrs { max_attrs: cfg.max_inline_attrs }),
    ];

    // Add build check if not skipped
    if !cli.no_build {
        checks.insert(0, Box::new(BuildCheck { 
            timeout_seconds: cli.timeout_seconds,
            root: root.clone(),
        }));
    }

    // Run all checks in parallel
    let results = run_checks(&checks, &prod_files, &all_files, &cfg);

    // Display results
    print_results(&results, &cfg);

    // Score and status
    let (score, status) = compute_score(&results);
    let (crit, high, med, low) = count_by_severity(&results);

    println!("\n{}", "📊 COMPLIANCE SCORE".blue().bold());
    println!("Critical: {}  High: {}  Medium: {}  Low: {}", crit, high, med, low);
    println!("Score: {}/100  Status: {}", score, status);

    // Write reports
    let report_dir = cli.report_dir
        .unwrap_or_else(|| format!("{}/reports/compliance", root.display()));
    write_reports(&report_dir, &results, score, &status)?;

    // CI-friendly exit logic
    let any_warn = high > 0 || med > 0 || low > 0;
    
    let mut fail = false;
    if cfg.ci.fail_on_critical && crit > 0 { 
        fail = true; 
    }
    if cfg.ci.fail_on_high && high > 0 { 
        fail = true; 
    }
    if score < cfg.ci.min_score { 
        fail = true; 
    }
    
    let exit_code = if fail {
        println!("\n{}", format!("❌ COMMIT REJECTED — fix {} critical, {} high issues", crit, high).red().bold());
        1
    } else if any_warn && cfg.ci.warn_exit_code != 0 {
        println!("\n{}", "⚠️  COMMIT ALLOWED WITH WARNINGS".yellow().bold());
        cfg.ci.warn_exit_code
    } else {
        println!("\n{}", "✅ COMMIT AUTHORIZED".green().bold());
        0
    };
    
    std::process::exit(exit_code);
}

/// Collect .rs files respecting .gitignore
fn collect_files(root: &Path, prod_only: bool) -> Vec<PathBuf> {
    fn is_prod(p: &Path) -> bool {
        let s = p.to_string_lossy();
        !(s.contains("/tests/") || s.contains("/test/") ||
          s.contains("/benches/") || s.contains("/bench/") ||
          s.contains("/examples/") || s.contains("/target/"))
    }
    
    let mut walker = WalkBuilder::new(root);
    walker
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .ignore(true);
    
    walker.build()
        .filter_map(|r| r.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter(|e| e.path().extension().map(|x| x == "rs").unwrap_or(false))
        .filter(|e| !prod_only || is_prod(e.path()))
        .map(DirEntry::into_path)
        .collect()
}

/// Run all checks in parallel
fn run_checks(
    checks: &[Box<dyn Check>],
    prod_files: &[PathBuf],
    all_files: &[PathBuf],
    cfg: &Config,
) -> Vec<CheckResult> {
    checks.par_iter()
        .map(|c| {
            // Use appropriate file set based on check
            let files = match c.name() {
                "todos" | "large_functions" | "inline_attrs" => all_files,
                _ => prod_files,
            };
            
            c.run(files, cfg).unwrap_or(CheckResult {
                name: c.name(),
                severity: Severity::CRITICAL,
                count: 1,
                message: format!("Check '{}' crashed", c.name()),
                details: vec![],
            })
        })
        .collect()
}

fn print_results(results: &[CheckResult], cfg: &Config) {
    println!("\n{}", "Check Results:".blue().bold());
    
    for r in results {
        let line = match r.severity {
            Severity::CRITICAL | Severity::HIGH => format!("{}", r.message.red()),
            Severity::MEDIUM | Severity::LOW => format!("{}", r.message.yellow()),
            Severity::OK => format!("{}", r.message.green()),
            Severity::SKIP => format!("{}", r.message.yellow()),
        };
        println!("{}", line);
        
        if cfg.details && !r.details.is_empty() {
            println!("  {} Top offenders:", "→".blue());
            for d in r.details.iter().take(cfg.top_n) {
                println!("    {}", d);
            }
        }
    }
}

fn compute_score(results: &[CheckResult]) -> (i32, String) {
    let (crit, high, med, low) = count_by_severity(results);
    let deduction = crit * 25 + high * 10 + med * 3 + low * 1;
    let mut score = 100i32.saturating_sub(deduction as i32);
    if score < 0 { score = 0; }
    
    let status = match score {
        90..=100 => "EXCELLENT",
        70..=89 => "GOOD",
        50..=69 => "NEEDS_IMPROVEMENT",
        _ => "CRITICAL",
    }.to_string();
    
    (score, status)
}

fn count_by_severity(results: &[CheckResult]) -> (usize, usize, usize, usize) {
    let mut crit = 0;
    let mut high = 0;
    let mut med = 0;
    let mut low = 0;
    
    for r in results {
        match r.severity {
            Severity::CRITICAL => crit += 1,
            Severity::HIGH => high += 1,
            Severity::MEDIUM => med += 1,
            Severity::LOW => low += 1,
            _ => {}
        }
    }
    
    (crit, high, med, low)
}

fn get_git_commit() -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
}

fn chrono_stamp() -> String {
    use std::time::UNIX_EPOCH;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    
    // Format: YYYYMMDD-HHMMSS (approximate)
    format!("{}", now)
}

#[derive(Serialize)]
struct JsonReport<'a> {
    timestamp: String,
    commit: String,
    violations: serde_json::Value,
    score: i32,
    status: &'a str,
    commit_authorized: bool,
}

fn write_reports(dir: &str, results: &[CheckResult], score: i32, status: &str) -> Result<()> {
    fs::create_dir_all(dir)?;
    
    let git_commit = get_git_commit().unwrap_or_else(|| "uncommitted".into());
    let stamp = chrono_stamp();
    
    // Text report
    let txt_path = format!("{}/compliance-report-{}-{}.txt", dir, git_commit, stamp);
    let mut txt = String::new();
    txt.push_str("ShrivenQuant Compliance Report\n");
    txt.push_str("===============================\n");
    txt.push_str(&format!("Generated: {}\nCommit: {}\n\n", stamp, git_commit));
    
    for r in results {
        let status_str = match r.severity {
            Severity::CRITICAL | Severity::HIGH => "FAIL",
            Severity::MEDIUM | Severity::LOW => "WARN",
            Severity::OK | Severity::SKIP => "OK",
        };
        txt.push_str(&format!("[{}] {} - {} (count: {})\n", 
            status_str, r.name, r.message, r.count));
    }
    
    let (crit, high, med, low) = count_by_severity(results);
    txt.push_str(&format!("\nViolations: Critical={} High={} Medium={} Low={}\n", 
        crit, high, med, low));
    txt.push_str(&format!("Score: {}/100  Status: {}\n", score, status));
    
    fs::write(&txt_path, txt)?;
    
    // JSON report
    let json_report = JsonReport {
        timestamp: stamp.clone(),
        commit: git_commit.clone(),
        violations: serde_json::json!({
            "critical": crit,
            "high": high,
            "medium": med,
            "low": low,
            "total": crit + high + med + low,
        }),
        score,
        status,
        commit_authorized: crit == 0,
    };
    
    let json_path = format!("{}/compliance-report-{}-{}.json", dir, git_commit, stamp);
    fs::write(&json_path, serde_json::to_string_pretty(&json_report)?)?;
    
    println!("\n{}", "📁 Reports saved:".blue());
    println!("  Text: {}", txt_path);
    println!("  JSON: {}", json_path);
    
    Ok(())
}

/* ========================= CHECKS ========================= */

/// Build check with timeout
struct BuildCheck {
    timeout_seconds: u64,
    root: PathBuf,
}

impl Check for BuildCheck {
    fn name(&self) -> &'static str { "build" }
    
    fn run(&self, _files: &[PathBuf], _cfg: &Config) -> Result<CheckResult> {
        let timeout = Duration::from_secs(self.timeout_seconds);
        let start = Instant::now();
        
        let mut child = Command::new("cargo")
            .args(["build", "--release", "--quiet"])
            .current_dir(&self.root)
            .spawn()?;
        
        loop {
            match child.try_wait()? {
                Some(status) if status.success() => {
                    return Ok(CheckResult {
                        name: self.name(),
                        severity: Severity::OK,
                        count: 0,
                        message: "✓ Release build successful".into(),
                        details: vec![],
                    });
                }
                Some(_) => {
                    return Ok(CheckResult {
                        name: self.name(),
                        severity: Severity::CRITICAL,
                        count: 1,
                        message: "❌ Release build failed".into(),
                        details: vec![],
                    });
                }
                None => {
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        return Ok(CheckResult {
                            name: self.name(),
                            severity: Severity::CRITICAL,
                            count: 1,
                            message: format!("❌ Build timeout after {}s", self.timeout_seconds),
                            details: vec![],
                        });
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }
}

/// Numeric casts without allow/expect
struct NumericCasts {
    max_files: usize,
}

impl Check for NumericCasts {
    fn name(&self) -> &'static str { "numeric_casts" }
    
    fn run(&self, files: &[PathBuf], cfg: &Config) -> Result<CheckResult> {
        let cast_re = Regex::new(r" as (u8|u16|u32|u64|u128|i8|i16|i32|i64|i128|f32|f64|usize|isize)\b")?;
        let allow_re = Regex::new(r"#\[(allow|expect)\(clippy::cast_")?;
        
        let violations: Vec<String> = files.par_iter()
            .filter_map(|p| {
                let text = fs::read_to_string(p).ok()?;
                if cast_re.is_match(&text) && !allow_re.is_match(&text) {
                    Some(p.display().to_string())
                } else {
                    None
                }
            })
            .collect();
        
        let count = violations.len();
        let (severity, message) = if count > self.max_files {
            (Severity::CRITICAL, 
             format!("❌ {} files with unannotated numeric casts (max {})", count, self.max_files))
        } else {
            (Severity::OK, 
             format!("✓ Cast usage acceptable ({} files)", count))
        };
        
        Ok(CheckResult {
            name: self.name(),
            severity,
            count,
            message,
            details: if cfg.details { violations.into_iter().take(cfg.top_n).collect() } else { vec![] },
        })
    }
}

/// Panic/unwrap/expect detection
struct PanicUnwrap;

impl Check for PanicUnwrap {
    fn name(&self) -> &'static str { "panic_unwrap" }
    
    fn run(&self, files: &[PathBuf], cfg: &Config) -> Result<CheckResult> {
        let re = Regex::new(r"panic!|\.unwrap\(\)|\.expect\(")?;
        
        let violations: Vec<String> = files.par_iter()
            .filter(|p| !cfg.allow.is_allowed(self.name(), p))
            .flat_map(|p| {
                let mut matches = Vec::new();
                if let Ok(text) = fs::read_to_string(p) {
                    for (line_num, line) in text.lines().enumerate() {
                        if re.is_match(line) {
                            matches.push(format!("{}:{}: {}", 
                                p.display(), 
                                line_num + 1, 
                                line.trim()
                            ));
                        }
                    }
                }
                matches
            })
            .collect();
        
        let count = violations.len();
        let (severity, message) = if count > 0 {
            (Severity::CRITICAL, 
             format!("❌ Found {} panic/unwrap/expect occurrences in production", count))
        } else {
            (Severity::OK, 
             "✓ No panic/unwrap/expect in production".into())
        };
        
        Ok(CheckResult {
            name: self.name(),
            severity,
            count,
            message,
            details: if cfg.details { violations.into_iter().take(cfg.top_n).collect() } else { vec![] },
        })
    }
}

/// std::HashMap usage check
struct StdHashMap;

impl Check for StdHashMap {
    fn name(&self) -> &'static str { "std_hashmap" }
    
    fn run(&self, files: &[PathBuf], cfg: &Config) -> Result<CheckResult> {
        let re = Regex::new(r"(^|[^:])std::collections::HashMap|use\s+std::collections::HashMap")?;
        
        let violations: Vec<String> = files.par_iter()
            .filter(|p| !cfg.allow.is_allowed(self.name(), p))
            .filter_map(|p| {
                let text = fs::read_to_string(p).ok()?;
                if re.is_match(&text) {
                    Some(p.display().to_string())
                } else {
                    None
                }
            })
            .collect();
        
        let count = violations.len();
        let (severity, message) = if count > 0 {
            (Severity::HIGH, 
             format!("❌ Found {} files using std::HashMap (prefer FxHashMap in hot paths)", count))
        } else {
            (Severity::OK, 
             "✓ No std::HashMap usage in prod paths".into())
        };
        
        Ok(CheckResult {
            name: self.name(),
            severity,
            count,
            message,
            details: if cfg.details { violations.into_iter().take(cfg.top_n).collect() } else { vec![] },
        })
    }
}

/// Float money detection
struct FloatMoney;

impl Check for FloatMoney {
    fn name(&self) -> &'static str { "float_money" }
    
    fn run(&self, files: &[PathBuf], cfg: &Config) -> Result<CheckResult> {
        let float_re = Regex::new(r": f(32|64).*price|: f(32|64).*amount|price: f(32|64)|amount: f(32|64)")?;
        let deserialize_re = Regex::new(r"#\[derive.*Deserialize")?;
        
        let violations: Vec<String> = files.par_iter()
            .filter(|p| !cfg.allow.is_allowed(self.name(), p))
            .filter_map(|p| {
                let path_str = p.to_string_lossy();
                // Skip exempted paths
                if path_str.contains("/display/") || 
                   path_str.contains("/features/") ||
                   path_str.contains("/loaders/") ||
                   path_str.contains("/adapters/") ||
                   path_str.contains("/apis/") {
                    return None;
                }
                
                let text = fs::read_to_string(p).ok()?;
                if float_re.is_match(&text) && !deserialize_re.is_match(&text) {
                    Some(p.display().to_string())
                } else {
                    None
                }
            })
            .collect();
        
        let count = violations.len();
        let (severity, message) = if count > 0 {
            (Severity::CRITICAL, 
             format!("❌ {} files use float for money in internal code", count))
        } else {
            (Severity::OK, 
             "✓ No floating point money in internal calculations".into())
        };
        
        Ok(CheckResult {
            name: self.name(),
            severity,
            count,
            message,
            details: if cfg.details { violations.into_iter().take(cfg.top_n).collect() } else { vec![] },
        })
    }
}

/// TODO/FIXME markers
struct Todos {
    max_todos: usize,
}

impl Check for Todos {
    fn name(&self) -> &'static str { "todos" }
    
    fn run(&self, files: &[PathBuf], cfg: &Config) -> Result<CheckResult> {
        let re = Regex::new(r"TODO|FIXME|HACK|XXX")?;
        
        let violations: Vec<String> = files.par_iter()
            .filter(|p| !cfg.allow.is_allowed(self.name(), p))
            .flat_map(|p| {
                let mut matches = Vec::new();
                if let Ok(text) = fs::read_to_string(p) {
                    for (line_num, line) in text.lines().enumerate() {
                        if re.is_match(line) {
                            matches.push(format!("{}:{}: {}", 
                                p.display(), 
                                line_num + 1, 
                                line.trim()
                            ));
                        }
                    }
                }
                matches
            })
            .collect();
        
        let count = violations.len();
        let (severity, message) = if count > self.max_todos {
            (Severity::CRITICAL, 
             format!("❌ Found {} TODO/FIXME/HACK/XXX markers", count))
        } else {
            (Severity::OK, 
             "✓ No outstanding TODO/FIXME markers".into())
        };
        
        Ok(CheckResult {
            name: self.name(),
            severity,
            count,
            message,
            details: if cfg.details { violations.into_iter().take(cfg.top_n).collect() } else { vec![] },
        })
    }
}

/// Ignored errors check
struct IgnoredErrors {
    max_err_ignore: usize,
}

impl Check for IgnoredErrors {
    fn name(&self) -> &'static str { "ignored_errors" }
    
    fn run(&self, files: &[PathBuf], cfg: &Config) -> Result<CheckResult> {
        let re = Regex::new(r"Err\(_\)")?;
        
        let violations: Vec<String> = files.par_iter()
            .filter(|p| !cfg.allow.is_allowed(self.name(), p))
            .flat_map(|p| {
                let mut matches = Vec::new();
                if let Ok(text) = fs::read_to_string(p) {
                    for (line_num, line) in text.lines().enumerate() {
                        if re.is_match(line) {
                            matches.push(format!("{}:{}: {}", 
                                p.display(), 
                                line_num + 1, 
                                line.trim()
                            ));
                        }
                    }
                }
                matches
            })
            .collect();
        
        let count = violations.len();
        let (severity, message) = if count > self.max_err_ignore {
            (Severity::CRITICAL, 
             format!("❌ Found {} ignored error patterns (Err(_))", count))
        } else {
            (Severity::OK, 
             "✓ No ignored error patterns in prod".into())
        };
        
        Ok(CheckResult {
            name: self.name(),
            severity,
            count,
            message,
            details: if cfg.details { violations.into_iter().take(cfg.top_n).collect() } else { vec![] },
        })
    }
}

/// Clone usage check
struct Clones;

impl Check for Clones {
    fn name(&self) -> &'static str { "clones" }
    
    fn run(&self, files: &[PathBuf], cfg: &Config) -> Result<CheckResult> {
        let violations: Vec<String> = files.par_iter()
            .filter(|p| !cfg.allow.is_allowed(self.name(), p))
            .flat_map(|p| {
                let mut matches = Vec::new();
                if let Ok(text) = fs::read_to_string(p) {
                    for (line_num, line) in text.lines().enumerate() {
                        if line.contains(".clone()") {
                            matches.push(format!("{}:{}: {}", 
                                p.display(), 
                                line_num + 1, 
                                line.trim()
                            ));
                        }
                    }
                }
                matches
            })
            .collect();
        
        let count = violations.len();
        let (severity, message) = if count > 50 {
            (Severity::MEDIUM, 
             format!("⚠️  {} clone() calls in prod — review hot paths", count))
        } else {
            (Severity::OK, 
             format!("✓ Clone usage reasonable ({} calls)", count))
        };
        
        Ok(CheckResult {
            name: self.name(),
            severity,
            count,
            message,
            details: if cfg.details { violations.into_iter().take(cfg.top_n).collect() } else { vec![] },
        })
    }
}

/// String allocations check
struct StringAllocs;

impl Check for StringAllocs {
    fn name(&self) -> &'static str { "string_allocs" }
    
    fn run(&self, files: &[PathBuf], cfg: &Config) -> Result<CheckResult> {
        let re = Regex::new(r"to_string\(\)|format!|String::from")?;
        
        let violations: Vec<String> = files.par_iter()
            .filter(|p| !cfg.allow.is_allowed(self.name(), p))
            .flat_map(|p| {
                let mut matches = Vec::new();
                if let Ok(text) = fs::read_to_string(p) {
                    for (line_num, line) in text.lines().enumerate() {
                        if re.is_match(line) {
                            matches.push(format!("{}:{}: {}", 
                                p.display(), 
                                line_num + 1, 
                                line.trim()
                            ));
                        }
                    }
                }
                matches
            })
            .collect();
        
        let count = violations.len();
        let (severity, message) = if count > 100 {
            (Severity::MEDIUM, 
             format!("⚠️  {} string allocation sites — avoid in hot paths", count))
        } else {
            (Severity::OK, 
             format!("✓ String allocations reasonable ({} sites)", count))
        };
        
        Ok(CheckResult {
            name: self.name(),
            severity,
            count,
            message,
            details: if cfg.details { violations.into_iter().take(cfg.top_n).collect() } else { vec![] },
        })
    }
}

/// Underscore variable abuse check
struct UnderscoreAbuse {
    max_abuse: usize,
}

impl Check for UnderscoreAbuse {
    fn name(&self) -> &'static str { "underscore_abuse" }
    
    fn run(&self, files: &[PathBuf], cfg: &Config) -> Result<CheckResult> {
        let underscore_re = Regex::new(r"let _[a-zA-Z0-9_]*\s*=")?;
        let exempt_re = Regex::new(r"_phantom|_guard|_lock")?;
        
        let violations: Vec<String> = files.par_iter()
            .filter(|p| !cfg.allow.is_allowed(self.name(), p))
            .flat_map(|p| {
                let mut matches = Vec::new();
                if let Ok(text) = fs::read_to_string(p) {
                    for (line_num, line) in text.lines().enumerate() {
                        if underscore_re.is_match(line) && !exempt_re.is_match(line) {
                            matches.push(format!("{}:{}: {}", 
                                p.display(), 
                                line_num + 1, 
                                line.trim()
                            ));
                        }
                    }
                }
                matches
            })
            .collect();
        
        let count = violations.len();
        let (severity, message) = if count > self.max_abuse {
            (Severity::CRITICAL, 
             format!("❌ {} lazy underscore variable usages", count))
        } else {
            (Severity::OK, 
             "✓ Underscore usage acceptable".into())
        };
        
        Ok(CheckResult {
            name: self.name(),
            severity,
            count,
            message,
            details: if cfg.details { violations.into_iter().take(cfg.top_n).collect() } else { vec![] },
        })
    }
}

/// Large functions check (>50 lines)
struct LargeFunctions {
    max_large: usize,
}

impl Check for LargeFunctions {
    fn name(&self) -> &'static str { "large_functions" }
    
    fn run(&self, files: &[PathBuf], cfg: &Config) -> Result<CheckResult> {
        let fn_start_re = Regex::new(r"^\s*(pub\s+)?fn\s+")?;
        let fn_end_re = Regex::new(r"^\s*\}\s*$")?;
        
        let file_counts: Vec<_> = files.par_iter()
            .filter(|p| !cfg.allow.is_allowed(self.name(), p))
            .filter_map(|p| {
                if let Ok(text) = fs::read_to_string(p) {
                    let lines: Vec<&str> = text.lines().collect();
                    let mut in_fn = false;
                    let mut fn_start_line = 0;
                    let mut large_count = 0;
                    
                    for (i, line) in lines.iter().enumerate() {
                        if fn_start_re.is_match(line) {
                            in_fn = true;
                            fn_start_line = i;
                        } else if in_fn && fn_end_re.is_match(line) {
                            if i - fn_start_line > 50 {
                                large_count += 1;
                            }
                            in_fn = false;
                        }
                    }
                    
                    if large_count > 0 {
                        return Some((p.display().to_string(), large_count));
                    }
                }
                None
            })
            .collect();
        
        let total_count: usize = file_counts.iter().map(|(_, c)| c).sum();
        let (severity, message) = if total_count > self.max_large {
            (Severity::LOW, 
             format!("⚠️  Found {} functions >50 lines", total_count))
        } else {
            (Severity::OK, 
             "✓ Function lengths look reasonable".into())
        };
        
        let details = if cfg.details {
            file_counts.into_iter()
                .take(cfg.top_n)
                .map(|(path, count)| format!("{}: {} large functions", path, count))
                .collect()
        } else {
            vec![]
        };
        
        Ok(CheckResult {
            name: self.name(),
            severity,
            count: total_count,
            message,
            details,
        })
    }
}

/// Inline attributes check
struct InlineAttrs {
    max_attrs: usize,
}

impl Check for InlineAttrs {
    fn name(&self) -> &'static str { "inline_attrs" }
    
    fn run(&self, files: &[PathBuf], cfg: &Config) -> Result<CheckResult> {
        let violations: Vec<String> = files.par_iter()
            .filter(|p| !cfg.allow.is_allowed(self.name(), p))
            .flat_map(|p| {
                let mut matches = Vec::new();
                if let Ok(text) = fs::read_to_string(p) {
                    for (line_num, line) in text.lines().enumerate() {
                        if line.contains("#[inline") {
                            matches.push(format!("{}:{}: {}", 
                                p.display(), 
                                line_num + 1, 
                                line.trim()
                            ));
                        }
                    }
                }
                matches
            })
            .collect();
        
        let count = violations.len();
        let (severity, message) = if count > self.max_attrs {
            (Severity::LOW, 
             format!("⚠️  {} #[inline] attributes (max {})", count, self.max_attrs))
        } else {
            (Severity::OK, 
             format!("✓ Inline attribute usage reasonable ({} uses)", count))
        };
        
        Ok(CheckResult {
            name: self.name(),
            severity,
            count,
            message,
            details: if cfg.details { violations.into_iter().take(cfg.top_n).collect() } else { vec![] },
        })
    }
}

/// Magic numbers check (4+ digit numbers outside const/static)
struct MagicNumbers {
    max_numbers: usize,
}

impl Check for MagicNumbers {
    fn name(&self) -> &'static str { "magic_numbers" }
    
    fn run(&self, files: &[PathBuf], cfg: &Config) -> Result<CheckResult> {
        let num_re = Regex::new(r"\b\d{4,}\b")?;
        let const_static_re = Regex::new(r"^\s*(const|static)\b")?;
        
        let violations: Vec<String> = files.par_iter()
            .filter(|p| !cfg.allow.is_allowed(self.name(), p))
            .flat_map(|p| {
                let mut matches = Vec::new();
                if let Ok(text) = fs::read_to_string(p) {
                    for (line_num, line) in text.lines().enumerate() {
                        if !const_static_re.is_match(line) && num_re.is_match(line) {
                            matches.push(format!("{}:{}: {}", 
                                p.display(), 
                                line_num + 1, 
                                line.trim()
                            ));
                        }
                    }
                }
                matches
            })
            .collect();
        
        let count = violations.len();
        let (severity, message) = if count > self.max_numbers {
            (Severity::LOW, 
             format!("⚠️  {} potential magic numbers — prefer named constants (max {})", count, self.max_numbers))
        } else {
            (Severity::OK, 
             format!("✓ Magic numbers acceptable ({} found)", count))
        };
        
        Ok(CheckResult {
            name: self.name(),
            severity,
            count,
            message,
            details: if cfg.details { violations.into_iter().take(cfg.top_n).collect() } else { vec![] },
        })
    }
}

/// Warning suppressions check
struct WarningSuppressions {
    max_suppressions: usize,
}

impl Check for WarningSuppressions {
    fn name(&self) -> &'static str { "warning_suppressions" }
    
    fn run(&self, files: &[PathBuf], cfg: &Config) -> Result<CheckResult> {
        let re = Regex::new(r"#\s*\[\s*allow\s*\(")?;
        
        let violations: Vec<String> = files.par_iter()
            .filter(|p| !cfg.allow.is_allowed(self.name(), p))
            .flat_map(|p| {
                let mut matches = Vec::new();
                if let Ok(text) = fs::read_to_string(p) {
                    for (line_num, line) in text.lines().enumerate() {
                        if re.is_match(line) {
                            matches.push(format!("{}:{}: {}", 
                                p.display(), 
                                line_num + 1, 
                                line.trim()
                            ));
                        }
                    }
                }
                matches
            })
            .collect();
        
        let count = violations.len();
        let (severity, message) = if count > self.max_suppressions {
            (Severity::LOW, 
             format!("⚠️  {} #[allow(...)] usages — fix root causes when possible (max {})", count, self.max_suppressions))
        } else {
            (Severity::OK, 
             format!("✓ Warning suppressions acceptable ({} found)", count))
        };
        
        Ok(CheckResult {
            name: self.name(),
            severity,
            count,
            message,
            details: if cfg.details { violations.into_iter().take(cfg.top_n).collect() } else { vec![] },
        })
    }
}

/// Doc duplication check
struct DocDupes {
    max_dupes: usize,
}

impl Check for DocDupes {
    fn name(&self) -> &'static str { "doc_dupes" }
    
    fn run(&self, files: &[PathBuf], cfg: &Config) -> Result<CheckResult> {
        let mut doc_lines: Vec<String> = files.par_iter()
            .filter(|p| !cfg.allow.is_allowed(self.name(), p))
            .flat_map(|p| {
                let mut lines = Vec::new();
                if let Ok(text) = fs::read_to_string(p) {
                    for line in text.lines() {
                        if let Some(rest) = line.strip_prefix("///") {
                            let s = rest.trim();
                            if !s.is_empty() && !s.starts_with('#') && !s.eq_ignore_ascii_case("safety") {
                                lines.push(s.to_string());
                            }
                        }
                    }
                }
                lines
            })
            .collect();
        
        if doc_lines.is_empty() {
            return Ok(CheckResult {
                name: self.name(),
                severity: Severity::OK,
                count: 0,
                message: "✓ No doc lines to analyze".into(),
                details: vec![],
            });
        }
        
        doc_lines.sort_unstable();
        let mut max_dup = 1usize;
        let mut cur = 1usize;
        for w in doc_lines.windows(2) {
            if w[0] == w[1] { 
                cur += 1; 
            } else { 
                max_dup = max_dup.max(cur); 
                cur = 1; 
            }
        }
        max_dup = max_dup.max(cur);
        
        let (severity, message) = if max_dup > self.max_dupes {
            (Severity::LOW, 
             format!("⚠️  Identical doc line repeats {}× (max {}) — consider refactoring docs", max_dup, self.max_dupes))
        } else {
            (Severity::OK, 
             format!("✓ Doc duplication acceptable (max repeat {}×)", max_dup))
        };
        
        Ok(CheckResult {
            name: self.name(),
            severity,
            count: max_dup,
            message,
            details: vec![],
        })
    }
}
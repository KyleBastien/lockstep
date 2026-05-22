use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use lockstep_config::Config;
use lockstep_core::{Finding, Report};
use lockstep_engine::{run, EngineOptions};

#[derive(Debug, Parser)]
#[command(
    name = "lockstep",
    version,
    about = "JS→TS migration syntax-equivalence checker"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Verify that touched .ts/.tsx files preserve the syntactic behavior of
    /// their .js counterparts on the default branch.
    Verify {
        /// Explicit paths to verify. Omit to check every touched .ts/.tsx.
        paths: Vec<PathBuf>,
        /// Override the default branch from config (e.g. `master`).
        #[arg(long)]
        base: Option<String>,
        /// Override the config file location.
        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Human)]
        format: Format,
        /// Severity at which the CLI exits non-zero.
        #[arg(long, value_enum, default_value_t = FailOn::Error)]
        fail_on: FailOn,
        /// Force granular reporting even if config disables it.
        #[arg(long)]
        report_all_findings: bool,
        /// Dump the normalized base/head source for each pair under
        /// `.lockstep/debug/` for manual inspection.
        #[arg(long)]
        verbose: bool,
        /// Repository root. Defaults to current directory.
        #[arg(long, value_name = "PATH")]
        repo: Option<PathBuf>,
    },
    /// Write a default `.lockstep/config.toml` if one doesn't exist.
    Init,
    /// Print human prose for a Finding category.
    Explain {
        /// Category name (kind_mismatch, token_mismatch, arity_mismatch,
        /// dropped_statement, stripped_ts_construct, parse_error).
        category: String,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Format {
    Human,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum FailOn {
    Error,
    Warn,
    Info,
}

fn main() -> ExitCode {
    init_tracing();
    dispatch(Cli::parse().cmd)
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("LOCKSTEP_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();
}

fn dispatch(cmd: Cmd) -> ExitCode {
    match cmd {
        Cmd::Verify {
            paths,
            base,
            config,
            format,
            fail_on,
            report_all_findings,
            verbose,
            repo,
        } => handle_verify(VerifyArgs {
            paths,
            base,
            config,
            format,
            fail_on,
            report_all_findings,
            verbose,
            repo,
        }),
        Cmd::Init => handle_init(),
        Cmd::Explain { category } => handle_explain(&category),
    }
}

fn handle_verify(args: VerifyArgs) -> ExitCode {
    match run_verify(args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("lockstep: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn handle_init() -> ExitCode {
    match run_init() {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("lockstep init: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn handle_explain(category: &str) -> ExitCode {
    match run_explain(category) {
        Some(s) => {
            println!("{s}");
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("unknown category: {category}");
            ExitCode::from(2)
        }
    }
}

struct VerifyArgs {
    paths: Vec<PathBuf>,
    base: Option<String>,
    config: Option<PathBuf>,
    format: Format,
    fail_on: FailOn,
    report_all_findings: bool,
    verbose: bool,
    repo: Option<PathBuf>,
}

fn run_verify(args: VerifyArgs) -> Result<ExitCode> {
    let repo_root = args
        .repo
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let config_path = args
        .config
        .unwrap_or_else(|| repo_root.join(".lockstep").join("config.toml"));
    let mut cfg = Config::load(&config_path).map_err(|e| anyhow::anyhow!("config: {e}"))?;
    if let Some(b) = args.base.clone() {
        cfg.default_branch = b;
    }
    if args.report_all_findings {
        cfg.report_all_findings = true;
    }

    let dump_dir = if args.verbose {
        Some(repo_root.join(".lockstep").join("debug"))
    } else {
        None
    };

    let opts = EngineOptions {
        repo_root: repo_root.clone(),
        base_ref_override: args.base,
        explicit_paths: args.paths,
        dump_normalized_to: dump_dir,
    };
    let report = run(&cfg, &opts).map_err(|e| anyhow::anyhow!("engine: {e}"))?;

    match args.format {
        Format::Human => print_human(&report),
        Format::Json => {
            let s = serde_json::to_string_pretty(&report)?;
            println!("{s}");
        }
    }

    Ok(exit_for(&report, args.fail_on))
}

fn run_init() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let dir = cwd.join(".lockstep");
    std::fs::create_dir_all(&dir)?;
    let target = dir.join("config.toml");
    if target.exists() {
        println!(
            "lockstep: {} already exists; not overwriting",
            target.display()
        );
        return Ok(());
    }
    std::fs::write(&target, DEFAULT_CONFIG_TOML)?;
    println!("lockstep: wrote {}", target.display());
    Ok(())
}

fn run_explain(category: &str) -> Option<String> {
    use lockstep_core::Category::*;
    let c = match category {
        "kind_mismatch" => KindMismatch,
        "token_mismatch" => TokenMismatch,
        "arity_mismatch" => ArityMismatch,
        "dropped_statement" => DroppedStatement,
        "stripped_ts_construct" => StrippedTsConstruct,
        "parse_error" => ParseError,
        _ => return None,
    };
    Some(c.explain().to_string())
}

fn print_human(report: &Report) {
    println!("{}", report.summary);
    println!("verdict: {}", report.verdict.kind.as_str());
    if !report.verdict.reason.is_empty() {
        println!("reason:  {}", report.verdict.reason);
    }
    println!(
        "counts:  error={} warn={} info={}",
        report.counts.error, report.counts.warn, report.counts.info
    );
    if report.findings.is_empty() {
        return;
    }
    println!();
    for (i, f) in report.findings.iter().enumerate() {
        print_finding(i + 1, f);
    }
}

fn print_finding(idx: usize, f: &Finding) {
    println!(
        "[{idx}] {} {} {}",
        f.severity.as_str(),
        f.category.as_str(),
        f.path.display()
    );
    println!("    {}", f.message);
    if let (Some(bs), Some(hs)) = (&f.base_snippet, &f.head_snippet) {
        println!("    --- base ---");
        for line in bs.lines() {
            println!("    {line}");
        }
        println!("    --- head ---");
        for line in hs.lines() {
            println!("    {line}");
        }
    }
    println!();
}

fn exit_for(report: &Report, fail_on: FailOn) -> ExitCode {
    let trip = match fail_on {
        FailOn::Error => report.counts.error > 0,
        FailOn::Warn => report.counts.error + report.counts.warn > 0,
        FailOn::Info => report.counts.total() > 0,
    };
    if trip {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

const DEFAULT_CONFIG_TOML: &str = r#"# lockstep configuration.
default_branch = "main"
allow_var_to_const_let = true
allow_formatting_diff = true
allow_enum_to_iife = false
allow_constructor_assigned_method_equivalence = true
allow_closure_cache_field_alias = false
allow_array_first_element_or_null = false
allow_array_first_element_or_null_loose = false
allow_nullish_widening = false
allow_null_undefined_swap = false
allow_iife_async_wrapper = false
allow_transient_cache_wrap = false
allow_request_field_narrowing = false
allow_async_propagation = false
allow_defensive_null_guard = false
allow_non_null_alias_local = false
allow_defensive_log_guard = false
defensive_log_guard_methods = ["debug", "info", "warn", "error", "trace", "log"]
allow_dead_defensive_optional_chain_removal = false
# v0.1.16: extends `allow_dead_defensive_optional_chain_removal` with a witness
# for chained values consumed only by logging methods. Entries are dotted
# method paths matched by exact-suffix against the callee's compact text
# (boundary must be `.` or start-of-text), so `"logger.error"` matches
# `logger.error`, `this.logger.error`, `self.logger.error`. Empty list (default)
# leaves the rule unchanged. Documented runtime-edge divergence: when the
# chained value is actually nullish at runtime, base interpolates "undefined"
# whereas head throws on the property access.
# Example: dead_defensive_log_consumer_methods = ["logger.error", "logger.warn", "console.error"]
dead_defensive_log_consumer_methods = []
allow_unknown_catch_narrowing = false
allow_promise_settled_discrimination = false
allow_pure_narrowing_helper = false
narrowing_helpers = []
# When `allow_pure_narrowing_helper = true`, the two flags below cascade on
# automatically. Set either to `true` explicitly to enable only that rule.
allow_helper_call_site_substitution = false
allow_destructure_then_narrow = false
# v0.1.16: accept composition of helper alias + optional-chain removal + `??`
# widening with a safe-default literal at one AST position. Diverges
# observably when the aliased base path is nullish at runtime — see
# `alias_helper_widening`.
allow_alias_helper_optional_chain_composition = false
report_all_findings = true
ignore = [
    "**/*.test.ts",
    "**/*.test.tsx",
    "**/*.spec.ts",
    "**/*.spec.tsx",
    "**/__snapshots__/**",
    "**/node_modules/**",
    "**/dist/**",
    "**/build/**",
]

# v0.1.15 helper-alias tables. Both default to empty; set per workspace.
#
# `narrowing_helpers_unwrap` maps an array-returning narrowing helper to the
# field it unwraps on the base shape. Enables Gap 2A: head
#   const RAW = SOURCE;
#   const LOCAL = HELPER(RAW);
# is paired with base `const ALIAS = SOURCE;` and registers
# `LOCAL ↔ ALIAS.<field>`. Example:
#   [narrowing_helpers_unwrap]
#   readAdminRows = "data"
[narrowing_helpers_unwrap]

# `narrowing_helpers_aliases` maps a zero-argument config-reader helper to
# the base expression path it reads. Enables Gap 2B: head
#   const LOCAL = HELPER();
# strips the declaration and substitutes head reads of `LOCAL[.X.Y…]` with
# base reads of `<path>[.X.Y…]`. Example:
#   [narrowing_helpers_aliases]
#   readCdnConfig = "config.cdn_config?"
[narrowing_helpers_aliases]
"#;

#[cfg(test)]
mod tests {
    use super::DEFAULT_CONFIG_TOML;
    use lockstep_config::Config;

    #[test]
    fn default_config_toml_parses() {
        let parsed: Config =
            toml::from_str(DEFAULT_CONFIG_TOML).expect("default config TOML should parse cleanly");
        assert_eq!(parsed.default_branch, "main");
        assert!(parsed.narrowing_helpers_unwrap.is_empty());
        assert!(parsed.narrowing_helpers_aliases.is_empty());
    }
}

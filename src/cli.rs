use crate::ledger::{safe_truncate_str, MemoriaLedger};
use crate::viewer::generate_html_report;
use crate::wiretap::{run_command_wiretap, run_mcp_stdio_wiretap};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;

#[derive(Parser, Debug)]
#[command(
    name = "memoria",
    about = "Memoria: The Black Box for Your Black Box — Out-of-band immutable action ledger for AI agents.",
    version = "0.3.0"
)]
pub struct Cli {
    #[arg(short, long, default_value = "default", global = true)]
    pub project: String,

    #[arg(short, long, default_value = "session_0", global = true)]
    pub session: String,

    #[arg(long, global = true)]
    pub db: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Wrap and wiretap a CLI command
    Wrap {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        cmd: Vec<String>,
    },
    /// Wrap and wiretap an MCP stdio server
    Mcp {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        cmd: Vec<String>,
    },
    /// Query raw action index (ground reality)
    Actions {
        #[arg(short = 'n', long, default_value_t = 10)]
        limit: usize,

        #[arg(long)]
        json: bool,
    },
    /// Query stage milestones and provenance
    Context {
        #[arg(short = 'n', long, default_value_t = 5)]
        limit: usize,

        #[arg(long)]
        json: bool,
    },
    /// Record a high-level project stage
    Stage {
        stage_name: String,
        step: String,

        #[arg(short, long, default_value = "success")]
        outcome: String,

        #[arg(long)]
        notes: Option<String>,

        /// Certify this milestone as human-verified
        #[arg(long)]
        human: bool,

        /// Explicit actor identity (defaults to 'human' if --human is passed, otherwise 'agent')
        #[arg(long)]
        actor: Option<String>,
    },
    /// Verify cryptographic hash chain integrity
    Verify,
    /// Generate an air-gapped HTML audit report
    Export {
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Supervise an untrusted agent with physical verification and issue tracking
    Run {
        /// GitHub Issue number to link and audit
        #[arg(long)]
        issue: Option<String>,

        /// Independent verification command to execute on host post-run
        #[arg(long)]
        verify_cmd: Option<String>,

        /// Require at least one passing test execution recorded in the physical ledger
        #[arg(long)]
        require_tests: bool,

        /// Automatically close GitHub issue upon verified audit pass
        #[arg(long)]
        auto_close: bool,

        /// Command to execute under supervision
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        cmd: Vec<String>,
    },
}

pub fn run() {
    let cli = Cli::parse();
    let db_path = cli.db;
    let project = &cli.project;
    let session = &cli.session;

    match cli.command {
        Commands::Wrap { mut cmd } => {
            if !cmd.is_empty() && cmd[0] == "--" {
                cmd.remove(0);
            }
            if cmd.is_empty() {
                eprintln!("Error: No command specified to wrap. Usage: memoria wrap -- <command> [args...]");
                process::exit(1);
            }
            match run_command_wiretap(&cmd, project, session, db_path) {
                Ok(code) => process::exit(code),
                Err(e) => {
                    eprintln!("Error executing wiretap: {}", e);
                    process::exit(1);
                }
            }
        }
        Commands::Mcp { mut cmd } => {
            if !cmd.is_empty() && cmd[0] == "--" {
                cmd.remove(0);
            }
            if cmd.is_empty() {
                eprintln!("Error: No MCP server command specified. Usage: memoria mcp -- <server_command> [args...]");
                process::exit(1);
            }
            match run_mcp_stdio_wiretap(&cmd, project, session, db_path) {
                Ok(code) => process::exit(code),
                Err(e) => {
                    eprintln!("Error in MCP wiretap: {}", e);
                    process::exit(1);
                }
            }
        }
        Commands::Actions { limit, json } => {
            let ledger = match MemoriaLedger::new(db_path) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Database error: {}", e);
                    process::exit(1);
                }
            };
            let actions = match ledger.get_recent_actions(project, limit) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("Failed to fetch actions: {}", e);
                    process::exit(1);
                }
            };

            if json {
                println!("{}", serde_json::to_string_pretty(&actions).unwrap_or_default());
            } else {
                if actions.is_empty() {
                    println!("[memoria] No recorded actions found for project '{}'.", project);
                }
                for a in &actions {
                    let exit_str = if a.exit_code == 0 {
                        "OK".to_string()
                    } else {
                        format!("ERR({})", a.exit_code)
                    };
                    let ts = if a.timestamp_start.len() >= 19 {
                        &a.timestamp_start[..19]
                    } else {
                        &a.timestamp_start
                    };
                    println!(
                        "[{}] #{} {} ➔ {} ({}, {:.1}ms)",
                        ts, a.id, a.action_type, a.target, exit_str, a.duration_ms
                    );
                    if let Some(snippet) = &a.result_snippet {
                        let clean = snippet.trim().replace('\n', " ");
                        let truncated = safe_truncate_str(&clean, 77);
                        if truncated.len() < clean.len() {
                            println!("   ↳ {}...", truncated);
                        } else {
                            println!("   ↳ {}", truncated);
                        }
                    }
                }
            }
        }
        Commands::Context { limit, json } => {
            let ledger = match MemoriaLedger::new(db_path) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Database error: {}", e);
                    process::exit(1);
                }
            };
            let stages = match ledger.get_context(project, limit) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to fetch context: {}", e);
                    process::exit(1);
                }
            };

            if json {
                println!("{}", serde_json::to_string_pretty(&stages).unwrap_or_default());
            } else {
                if stages.is_empty() {
                    println!("[memoria] No stages recorded yet for project '{}'.", project);
                }
                for s in &stages {
                    let ts = if s.last_updated.len() >= 19 {
                        &s.last_updated[..19]
                    } else {
                        &s.last_updated
                    };
                    let actor_badge = if s.actor == "human" {
                        "[HUMAN CERTIFIED]"
                    } else {
                        "[AGENT]"
                    };
                    println!(
                        "[{}] #{} [{}] {} (outcome: {}, actor: {})",
                        ts,
                        s.id,
                        s.stage_name.to_uppercase(),
                        s.step_description,
                        s.outcome,
                        actor_badge
                    );
                }
            }
        }
        Commands::Stage {
            stage_name,
            step,
            outcome,
            notes,
            human,
            actor,
        } => {
            let ledger = match MemoriaLedger::new(db_path) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Database error: {}", e);
                    process::exit(1);
                }
            };

            let actor_str = match (human, actor) {
                (_, Some(a)) => a,
                (true, None) => "human".to_string(),
                (false, None) => {
                    if std::env::var("BLACKBOX_REQUIRE_HUMAN_STAGE").is_ok() || std::env::var("MEMORIA_REQUIRE_HUMAN_STAGE").is_ok() {
                        eprintln!("[blackbox] ACCESS DENIED: BLACKBOX_REQUIRE_HUMAN_STAGE is set. Stages must be recorded with --human.");
                        process::exit(1);
                    }
                    "agent".to_string()
                }
            };

            match ledger.commit_stage(
                project,
                session,
                &stage_name,
                &step,
                &outcome,
                &[],
                notes.as_deref(),
                &actor_str,
            ) {
                Ok(sid) => {
                    println!(
                        "[memoria] Stage recorded: id={} project='{}' stage='{}' actor='{}'",
                        sid, project, stage_name, actor_str
                    );
                }
                Err(e) => {
                    eprintln!("Failed to record stage: {}", e);
                    process::exit(1);
                }
            }
        }
        Commands::Verify => {
            let ledger = match MemoriaLedger::new(db_path) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Database error: {}", e);
                    process::exit(1);
                }
            };
            match ledger.verify_integrity(project) {
                Ok((true, count, _)) => {
                    println!(
                        "[memoria] INTEGRITY VERIFIED: {} blocks in hash chain for '{}'. Zero tampering detected.",
                        count, project
                    );
                }
                Ok((false, _, Some(err))) => {
                    eprintln!("[memoria] ❌ TAMPERING DETECTED: {}", err);
                    process::exit(1);
                }
                Ok((false, _, None)) => {
                    eprintln!("[memoria] ❌ TAMPERING DETECTED: Verification failed.");
                    process::exit(1);
                }
                Err(e) => {
                    eprintln!("Verification query error: {}", e);
                    process::exit(1);
                }
            }
        }
        Commands::Export { out } => match generate_html_report(project, db_path, out) {
            Ok(p) => {
                println!("[memoria] Flight report generated: {}", p.display());
            }
            Err(e) => {
                eprintln!("Failed to generate export: {}", e);
                process::exit(1);
            }
        },
        Commands::Run {
            issue,
            verify_cmd,
            require_tests,
            auto_close,
            mut cmd,
        } => {
            if !cmd.is_empty() && cmd[0] == "--" {
                cmd.remove(0);
            }
            if cmd.is_empty() {
                eprintln!("Error: No command specified to run. Usage: memoria run [options] -- <command> [args...]");
                process::exit(1);
            }

            let project_name = if let Some(iss) = &issue {
                format!("issue-{}", iss)
            } else {
                project.to_string()
            };

            let session_name = format!("run_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S"));

            let config = crate::runner::RunConfig {
                issue,
                verify_cmd,
                require_tests,
                auto_close,
                cmd,
                project: project_name,
                session: session_name,
                db_path,
            };

            match crate::runner::execute_supervised_run(config) {
                Ok(res) => {
                    if res.verification_passed {
                        process::exit(0);
                    } else {
                        process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("[memoria run] Fatal execution error: {}", e);
                    process::exit(1);
                }
            }
        },
    }
}

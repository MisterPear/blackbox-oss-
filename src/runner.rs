use crate::ledger::{safe_truncate_str, MemoriaLedger, TaskAction};
use crate::viewer::generate_html_report;
use crate::wiretap::run_command_wiretap;
use std::io;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct RunConfig {
    pub issue: Option<String>,
    pub verify_cmd: Option<String>,
    pub require_tests: bool,
    pub auto_close: bool,
    pub cmd: Vec<String>,
    pub project: String,
    pub session: String,
    pub db_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct RunAuditResult {
    pub agent_exit_code: i32,
    pub verification_passed: bool,
    pub failure_reasons: Vec<String>,
    pub total_actions: usize,
    pub report_path: Option<PathBuf>,
    pub markdown_summary: String,
}

pub fn execute_supervised_run(config: RunConfig) -> io::Result<RunAuditResult> {
    let ledger = MemoriaLedger::new(config.db_path.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    // 1. Pre-flight Setup
    println!("══════════════════════════════════════════════════════════════════════");
    println!(" [memoria run] INITIATING SUPERVISED AGENT EXECUTION");
    println!(" Target Project: {}", config.project);
    println!(" Session ID:     {}", config.session);
    if let Some(issue) = &config.issue {
        println!(" Binding Issue:  #{}", issue);
    }
    if let Some(verify) = &config.verify_cmd {
        println!(" Verify Command: {}", verify);
    }
    if config.require_tests {
        println!(" Audit Policy:   Strict (Physical test execution in ledger required)");
    }
    println!("══════════════════════════════════════════════════════════════════════");

    // Record lifecycle milestone
    let _ = ledger.commit_stage(
        &config.project,
        &config.session,
        "lifecycle",
        "Supervised agent execution initiated",
        "in_progress",
        &[],
        None,
        "supervisor",
    );

    // 2. Execute target command under physical wiretap
    println!("\n[memoria run] Spawning agent under physical wiretap shim...\n");
    let agent_exit_code = run_command_wiretap(
        &config.cmd,
        &config.project,
        &config.session,
        config.db_path.clone(),
    )?;

    println!("\n══════════════════════════════════════════════════════════════════════");
    println!(" [memoria run] AGENT TERMINATED (Exit Code: {})", agent_exit_code);
    println!(" INITIATING GROUND-TRUTH PHYSICAL VERIFICATION");
    println!(" Rule: Do not trust exit code 0 alone.");
    println!("══════════════════════════════════════════════════════════════════════\n");

    let mut failure_reasons = Vec::new();

    // 3. Physical Verification Step 1: Cryptographic Hash Chain Integrity
    let (is_chain_valid, block_count, chain_err) = ledger
        .verify_integrity(&config.project)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    if !is_chain_valid {
        failure_reasons.push(format!(
            "Cryptographic hash chain broken or tampered: {}",
            chain_err.unwrap_or_default()
        ));
    }

    // 4. Physical Verification Step 2: Agent Process Exit Code Check
    if agent_exit_code != 0 {
        failure_reasons.push(format!(
            "Agent process exited with non-zero error code ({})",
            agent_exit_code
        ));
    }

    // 5. Physical Verification Step 3: Action Ledger Inspection (--require-tests)
    let session_actions = ledger
        .get_session_actions(&config.project, &config.session)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    if config.require_tests {
        let has_passing_test = session_actions.iter().any(|a| {
            let target_lower = a.target.to_lowercase();
            let params_str = serde_json::to_string(&a.parameters).unwrap_or_default().to_lowercase();
            let is_test_cmd = target_lower.contains("test")
                || target_lower.contains("pytest")
                || target_lower.contains("vitest")
                || target_lower.contains("jest")
                || target_lower.contains("ctest")
                || params_str.contains("test")
                || params_str.contains("pytest")
                || params_str.contains("unittest");
            is_test_cmd && a.exit_code == 0
        });

        if !has_passing_test {
            failure_reasons.push(
                "Audit Failed: Agent claimed task completion, but zero passing test executions were recorded in the flight ledger."
                    .to_string(),
            );
        }
    }

    // 6. Physical Verification Step 4: Independent Verification Command (--verify-cmd)
    if let Some(verify_cmd) = &config.verify_cmd {
        println!("[memoria run] Executing independent host verification: `{}`", verify_cmd);
        let status = Command::new("sh")
            .arg("-c")
            .arg(verify_cmd)
            .status()?;

        if !status.success() {
            let code = status.code().unwrap_or(1);
            failure_reasons.push(format!(
                "Independent verification command `{}` failed with exit code {}",
                verify_cmd, code
            ));
        } else {
            println!("[memoria run] Host verification command passed cleanly.");
        }
    }

    let verification_passed = failure_reasons.is_empty();

    // 7. Commit Verification Milestone
    let outcome = if verification_passed { "success" } else { "failed" };
    let summary_note = if verification_passed {
        "All physical audit predicates passed. Hash chain sealed."
    } else {
        "Physical verification audit FAILED."
    };

    let _ = ledger.commit_stage(
        &config.project,
        &config.session,
        "verification",
        summary_note,
        outcome,
        &[],
        Some(&failure_reasons.join("; ")),
        "supervisor",
    );

    // 8. Generate Air-Gapped HTML Report
    let report_filename = format!("memoria_flight_report_{}.html", config.session);
    let html_out = generate_html_report(
        &config.project,
        config.db_path.clone(),
        Some(PathBuf::from(&report_filename)),
    ).ok();

    // 9. Format Markdown Summary
    let markdown_summary = generate_markdown_audit_report(
        &config,
        agent_exit_code,
        verification_passed,
        &failure_reasons,
        block_count,
        &session_actions,
    );

    // 10. Optional Issue Synchronization via gh CLI
    if let Some(issue_num) = &config.issue {
        sync_github_issue(issue_num, verification_passed, config.auto_close, &markdown_summary);
    }

    println!("\n══════════════════════════════════════════════════════════════════════");
    if verification_passed {
        println!(" [memoria run] ✅ AUDIT VERIFIED: ALL PHYSICAL PREDICATES SATISFIED");
        println!(" Cryptographic blocks verified: {}", block_count);
        if let Some(p) = &html_out {
            println!(" Flight report:                 {}", p.display());
        }
    } else {
        println!(" [memoria run] ❌ AUDIT FAILED: UNVERIFIED EXECUTION DETECTED");
        for r in &failure_reasons {
            println!("  • {}", r);
        }
    }
    println!("══════════════════════════════════════════════════════════════════════\n");

    Ok(RunAuditResult {
        agent_exit_code,
        verification_passed,
        failure_reasons,
        total_actions: session_actions.len(),
        report_path: html_out,
        markdown_summary,
    })
}

fn generate_markdown_audit_report(
    config: &RunConfig,
    exit_code: i32,
    passed: bool,
    failures: &[String],
    block_count: usize,
    actions: &[TaskAction],
) -> String {
    let status_badge = if passed {
        "**`STATUS: ✅ VERIFIED`**"
    } else {
        "**`STATUS: ❌ AUDIT FAILED`**"
    };

    let mut md = format!(
        "### Memoria Execution & Physical Flight Audit\n\n{}\n\n- **Project**: `{}`\n- **Session**: `{}`\n- **Process Exit Code**: `{}`\n- **Cryptographic Blocks**: `{}` blocks sealed to genesis\n\n",
        status_badge, config.project, config.session, exit_code, block_count
    );

    if !passed {
        md.push_str("#### ⚠️ Verification Failures:\n");
        for f in failures {
            md.push_str(&format!("* {}\n", f));
        }
        md.push('\n');
    }

    md.push_str("#### Physical Action Ledger\n\n");
    md.push_str("| # | Timestamp | Action | Target | Exit | Duration | SHA-256 Block |\n");
    md.push_str("|---|---|---|---|:---:|---:|---|\n");

    for a in actions {
        let ts = if a.timestamp_start.len() >= 19 {
            &a.timestamp_start[11..19]
        } else {
            &a.timestamp_start
        };
        let exit_str = if a.exit_code == 0 {
            "0 (OK)".to_string()
        } else {
            format!("ERR({})", a.exit_code)
        };
        let hash_short = if a.entry_hash.len() >= 12 {
            format!("`{}...`", &a.entry_hash[..10])
        } else {
            a.entry_hash.clone()
        };

        md.push_str(&format!(
            "| {} | {} | `{}` | `{}` | {} | {:.1}ms | {} |\n",
            a.id, ts, a.action_type, safe_truncate_str(&a.target, 25), exit_str, a.duration_ms, hash_short
        ));
    }

    md.push_str("\n<details>\n<summary>▶ Expand Captured Output Snippets</summary>\n\n");
    for a in actions {
        if let Some(snip) = &a.result_snippet {
            md.push_str(&format!(
                "**Action #{} (`{}`)**\n```text\n{}\n```\n\n",
                a.id,
                a.target,
                safe_truncate_str(snip.trim(), 400)
            ));
        }
    }
    md.push_str("</details>\n");

    md
}

fn sync_github_issue(issue_num: &str, passed: bool, auto_close: bool, summary: &str) {
    println!("[memoria run] Syncing audit findings with GitHub Issue #{}...", issue_num);

    let comment_res = Command::new("gh")
        .args(["issue", "comment", issue_num, "--body", summary])
        .output();

    match comment_res {
        Ok(out) if out.status.success() => {
            println!("[memoria run] Audit report posted to Issue #{}.", issue_num);
            if passed && auto_close {
                let close_res = Command::new("gh")
                    .args(["issue", "close", issue_num, "--comment", "Closed by Memoria supervisor following verified physical flight audit."])
                    .output();
                if let Ok(c_out) = close_res {
                    if c_out.status.success() {
                        println!("[memoria run] Issue #{} marked completed and closed.", issue_num);
                    }
                }
            }
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            println!("[memoria run] Note: `gh` CLI comment failed (status: {}): {}", out.status, err.trim());
        }
        Err(_) => {
            println!("[memoria run] Note: `gh` CLI not available on PATH. Skipping remote issue sync.");
        }
    }
}

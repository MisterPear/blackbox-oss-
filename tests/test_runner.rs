use blackbox::runner::{execute_supervised_run, RunConfig};
use tempfile::tempdir;

#[test]
fn test_supervised_run_rejects_exit_zero_without_tests() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test_runner_reject.db");

    // Agent executes an echo command claiming completion (exits 0)
    let config = RunConfig {
        issue: Some("99".to_string()),
        verify_cmd: None,
        require_tests: true, // Audit requirement
        auto_close: false,
        cmd: vec!["echo".to_string(), "I finished all work successfully!".to_string()],
        project: "test_proj".to_string(),
        session: "sess_reject".to_string(),
        db_path: Some(db_path),
    };

    let res = execute_supervised_run(config).unwrap();
    // Agent process exited 0...
    assert_eq!(res.agent_exit_code, 0);
    // ...BUT physical verification FAILED because no tests physically ran in the flight ledger!
    assert!(!res.verification_passed);
    assert!(res.failure_reasons.iter().any(|r| r.contains("zero passing test executions")));
    assert!(res.markdown_summary.contains("STATUS: ❌ AUDIT FAILED"));
}

#[test]
fn test_supervised_run_verifies_passing_tests() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test_runner_verify.db");

    // Agent executes a python test runner command
    let script = "print('Running test suite... ok')";
    let config = RunConfig {
        issue: Some("100".to_string()),
        verify_cmd: Some("true".to_string()), // Independent host verification succeeds
        require_tests: true,
        auto_close: false,
        cmd: vec!["python3".to_string(), "-c".to_string(), script.to_string(), "test_runner".to_string()],
        project: "test_proj".to_string(),
        session: "sess_pass".to_string(),
        db_path: Some(db_path),
    };

    let res = execute_supervised_run(config).unwrap();
    assert_eq!(res.agent_exit_code, 0);
    assert!(res.verification_passed);
    assert!(res.failure_reasons.is_empty());
    assert!(res.markdown_summary.contains("STATUS: ✅ VERIFIED"));
    assert!(res.markdown_summary.contains("Physical Action Ledger"));
}

#[test]
fn test_supervised_run_fails_if_verify_cmd_fails() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test_runner_verify_fail.db");

    let config = RunConfig {
        issue: None,
        verify_cmd: Some("false".to_string()), // Host verification fails (exit code 1)
        require_tests: false,
        auto_close: false,
        cmd: vec!["echo".to_string(), "hello".to_string()],
        project: "test_proj".to_string(),
        session: "sess_vfail".to_string(),
        db_path: Some(db_path),
    };

    let res = execute_supervised_run(config).unwrap();
    assert_eq!(res.agent_exit_code, 0);
    assert!(!res.verification_passed);
    assert!(res.failure_reasons.iter().any(|r| r.contains("Independent verification command `false` failed")));
}

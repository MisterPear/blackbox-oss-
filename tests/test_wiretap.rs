use blackbox::ledger::BlackBoxLedger;
use blackbox::viewer::generate_html_report;
use blackbox::wiretap::run_command_wiretap;
use tempfile::tempdir;

#[test]
fn test_command_wiretap_and_export() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test_wiretap.db");
    let html_out = tmp.path().join("report.html");

    // Execute an echo command through wiretap
    let cmd = vec!["echo".to_string(), "wiretap_verified_rust".to_string()];
    let code = run_command_wiretap(&cmd, "unit_test", "test_sess", Some(db_path.clone())).unwrap();
    assert_eq!(code, 0);

    // Verify recorded in ledger
    let ledger = BlackBoxLedger::new(Some(db_path.clone())).unwrap();
    let actions = ledger.get_recent_actions("unit_test", 10).unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].target, "echo");
    assert!(actions[0]
        .result_snippet
        .as_ref()
        .unwrap()
        .contains("wiretap_verified_rust"));
    // Verify bytes_out was counted accurately
    assert!(actions[0].bytes_out > 0);

    // Verify hash chain
    let (valid, count, err) = ledger.verify_integrity("unit_test").unwrap();
    assert!(valid);
    assert_eq!(count, 1);
    assert!(err.is_none());

    // Generate HTML report
    let out_file =
        generate_html_report("unit_test", Some(db_path), Some(html_out.clone())).unwrap();
    assert!(out_file.exists());
    let content = std::fs::read_to_string(out_file).unwrap();
    assert!(content.contains("wiretap_verified_rust"));
    assert!(content.contains("HASH CHAIN VALID"));
}

#[test]
fn test_exact_byte_accounting() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test_bytes.db");

    // Generate exactly 10,000 bytes of output
    let script = "print('A' * 10000, end='')";
    let cmd = vec!["python3".to_string(), "-c".to_string(), script.to_string()];
    let code = run_command_wiretap(&cmd, "bytes_test", "sess_b", Some(db_path.clone())).unwrap();
    assert_eq!(code, 0);

    let ledger = BlackBoxLedger::new(Some(db_path)).unwrap();
    let actions = ledger.get_recent_actions("bytes_test", 1).unwrap();
    assert_eq!(actions.len(), 1);
    // Should be exactly 10,000 bytes counted, not capped at 50 lines or 4096
    assert_eq!(actions[0].bytes_out, 10000);
}

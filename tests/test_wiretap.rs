use memoria::ledger::MemoriaLedger;
use memoria::viewer::generate_html_report;
use memoria::wiretap::run_command_wiretap;
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
    let ledger = MemoriaLedger::new(Some(db_path.clone())).unwrap();
    let actions = ledger.get_recent_actions("unit_test", 10).unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].target, "echo");
    assert!(actions[0]
        .result_snippet
        .as_ref()
        .unwrap()
        .contains("wiretap_verified_rust"));

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

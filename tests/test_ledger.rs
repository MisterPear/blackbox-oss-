use memoria::ledger::MemoriaLedger;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn test_ledger_chain_and_tamper_detection() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test_memoria.db");
    let ledger = MemoriaLedger::new(Some(db_path)).unwrap();

    // 1. Record two actions
    let id1 = ledger
        .record_action(
            "demo",
            "s1",
            "cli",
            "git status",
            json!({"cmd": "git status"}),
            "2026-11-23T12:00:00Z",
            "2026-11-23T12:00:01Z",
            1000.0,
            0,
            Some("clean"),
            5,
        )
        .unwrap();
    assert_eq!(id1, 1);

    let id2 = ledger
        .record_action(
            "demo",
            "s1",
            "cli",
            "pytest",
            json!({"cmd": "pytest"}),
            "2026-11-23T12:00:02Z",
            "2026-11-23T12:00:04Z",
            2000.0,
            0,
            Some("10 passed"),
            9,
        )
        .unwrap();
    assert_eq!(id2, 2);

    // 2. Verify clean chain
    let (valid, count, err) = ledger.verify_integrity("demo").unwrap();
    assert!(valid);
    assert_eq!(count, 2);
    assert!(err.is_none());

    // 3. Simulate an adversary tampering with entry #1
    let conn = ledger.get_connection().unwrap();
    conn.execute("UPDATE task_ledger SET exit_code=1 WHERE id=1", [])
        .unwrap();
    drop(conn);

    // 4. Verify tampering is caught
    let (valid_tampered, _, err_tampered) = ledger.verify_integrity("demo").unwrap();
    assert!(!valid_tampered);
    assert!(err_tampered.unwrap().contains("Hash mismatch"));
}

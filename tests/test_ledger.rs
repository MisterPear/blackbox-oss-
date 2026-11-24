use blackbox::ledger::BlackBoxLedger;
use serde_json::json;
use std::sync::Arc;
use std::thread;
use tempfile::tempdir;

#[test]
fn test_ledger_immutability_triggers() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test_immutability.db");
    let ledger = BlackBoxLedger::new(Some(db_path)).unwrap();

    let id = ledger
        .record_action(
            "test_proj",
            "sess1",
            "cli",
            "ls",
            json!({}),
            "2026-11-23T12:00:00Z",
            "2026-11-23T12:00:01Z",
            100.0,
            0,
            Some("file1.txt"),
            10,
        )
        .unwrap();
    assert_eq!(id, 1);

    let stage_id = ledger
        .commit_stage(
            "test_proj",
            "sess1",
            "init",
            "initial setup",
            "success",
            &[id],
            None,
            "human",
        )
        .unwrap();
    assert_eq!(stage_id, 1);

    let conn = ledger.get_connection().unwrap();

    // 1. Attempt UPDATE on task_ledger -> Must be aborted by trigger
    let update_res = conn.execute("UPDATE task_ledger SET exit_code=1 WHERE id=1", []);
    assert!(update_res.is_err());
    let err_msg = update_res.unwrap_err().to_string();
    assert!(
        err_msg.contains("BLACKBOX_SECURITY_VIOLATION") || err_msg.contains("MEMORIA_SECURITY_VIOLATION"),
        "Expected security violation, got: {}",
        err_msg
    );

    // 2. Attempt DELETE on task_ledger -> Must be aborted by trigger
    let delete_res = conn.execute("DELETE FROM task_ledger WHERE id=1", []);
    assert!(delete_res.is_err());
    let err_msg = delete_res.unwrap_err().to_string();
    assert!(
        err_msg.contains("BLACKBOX_SECURITY_VIOLATION") || err_msg.contains("MEMORIA_SECURITY_VIOLATION"),
        "Expected security violation, got: {}",
        err_msg
    );

    // 3. Attempt UPDATE on stage_ledger -> Must be aborted by trigger
    let update_stage_res = conn.execute("UPDATE stage_ledger SET outcome='failed' WHERE id=1", []);
    assert!(update_stage_res.is_err());
    let err_msg = update_stage_res.unwrap_err().to_string();
    assert!(
        err_msg.contains("BLACKBOX_SECURITY_VIOLATION") || err_msg.contains("MEMORIA_SECURITY_VIOLATION"),
        "Expected security violation, got: {}",
        err_msg
    );

    // 4. Attempt DELETE on stage_ledger -> Must be aborted by trigger
    let delete_stage_res = conn.execute("DELETE FROM stage_ledger WHERE id=1", []);
    assert!(delete_stage_res.is_err());
    let err_msg = delete_stage_res.unwrap_err().to_string();
    assert!(
        err_msg.contains("BLACKBOX_SECURITY_VIOLATION") || err_msg.contains("MEMORIA_SECURITY_VIOLATION"),
        "Expected security violation, got: {}",
        err_msg
    );

    // 5. If an adversary drops triggers and modifies data, hash chain verification still detects it
    conn.execute("DROP TRIGGER prevent_task_ledger_update", []).unwrap();
    conn.execute("UPDATE task_ledger SET exit_code=99 WHERE id=1", []).unwrap();
    drop(conn);

    let (valid, _, err) = ledger.verify_integrity("test_proj").unwrap();
    assert!(!valid);
    assert!(err.unwrap().contains("Hash mismatch"));
}

#[test]
fn test_utf8_multibyte_safety() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test_utf8.db");
    let ledger = BlackBoxLedger::new(Some(db_path)).unwrap();

    // Construct a 2500 character string of 4-byte unicode emojis and 3-byte CJK characters
    // Slicing at raw byte 2000 would land mid-character in a standard byte slice
    let complex_utf8 = "🦆🦀⚡🛡️🔥🚀".repeat(500);

    let res = ledger.record_action(
        "utf8_proj",
        "sess_utf8",
        "cli",
        "echo",
        json!({"test": "utf8"}),
        "2026-11-23T12:00:00Z",
        "2026-11-23T12:00:01Z",
        50.0,
        0,
        Some(&complex_utf8),
        complex_utf8.len() as i64,
    );

    assert!(res.is_ok(), "record_action panicked on multi-byte UTF-8 string!");

    let actions = ledger.get_recent_actions("utf8_proj", 1).unwrap();
    assert_eq!(actions.len(), 1);
    let snippet = actions[0].result_snippet.as_ref().unwrap();
    assert!(snippet.chars().count() <= 2000);
    assert!(snippet.starts_with("🦆🦀⚡"));
}

#[test]
fn test_concurrent_write_safety() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test_concurrent.db");
    let ledger = Arc::new(BlackBoxLedger::new(Some(db_path)).unwrap());

    let mut handles = Vec::new();
    let num_threads = 4;
    let writes_per_thread = 10;

    for t_id in 0..num_threads {
        let ledger_clone = Arc::clone(&ledger);
        handles.push(thread::spawn(move || {
            for i in 0..writes_per_thread {
                let session = format!("thread_{}", t_id);
                let target = format!("cmd_{}_{}", t_id, i);
                ledger_clone
                    .record_action(
                        "concurrent_proj",
                        &session,
                        "cli",
                        &target,
                        json!({"thread": t_id, "iter": i}),
                        "2026-11-23T12:00:00Z",
                        "2026-11-23T12:00:01Z",
                        10.0,
                        0,
                        Some("ok"),
                        2,
                    )
                    .unwrap();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let (valid, count, err) = ledger.verify_integrity("concurrent_proj").unwrap();
    assert!(valid, "Concurrent writes caused hash chain fork or corruption: {:?}", err);
    assert_eq!(count, num_threads * writes_per_thread);
}

#[test]
fn test_stage_actor_tiering() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test_stage_actor.db");
    let ledger = BlackBoxLedger::new(Some(db_path)).unwrap();

    ledger
        .commit_stage(
            "gov_proj",
            "sess1",
            "plan",
            "agent drafted plan",
            "success",
            &[],
            None,
            "agent",
        )
        .unwrap();

    ledger
        .commit_stage(
            "gov_proj",
            "sess1",
            "milestone_1",
            "human signoff",
            "approved",
            &[],
            Some("Reviewed and certified"),
            "human",
        )
        .unwrap();

    let stages = ledger.get_context("gov_proj", 10).unwrap();
    assert_eq!(stages.len(), 2);
    // Order is DESC: first is milestone_1 (human), second is plan (agent)
    assert_eq!(stages[0].stage_name, "milestone_1");
    assert_eq!(stages[0].actor, "human");
    assert_eq!(stages[1].stage_name, "plan");
    assert_eq!(stages[1].actor, "agent");
}

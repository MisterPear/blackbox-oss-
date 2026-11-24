use crate::ledger::{get_iso_timestamp, BlackBoxLedger};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

pub fn run_command_wiretap(
    cmd_args: &[String],
    project_name: &str,
    session_id: &str,
    db_path: Option<PathBuf>,
) -> io::Result<i32> {
    let ledger = BlackBoxLedger::new(db_path)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    let target_bin = cmd_args.first().cloned().unwrap_or_else(|| "unknown".to_string());
    let t_start_iso = get_iso_timestamp();
    let t_start = Instant::now();

    if cmd_args.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "No command provided"));
    }

    let mut child = match Command::new(&cmd_args[0])
        .args(&cmd_args[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(proc) => proc,
        Err(e) => {
            let t_end_iso = get_iso_timestamp();
            let duration_ms = t_start.elapsed().as_secs_f64() * 1000.0;
            let mut params_obj = serde_json::Map::new();
            params_obj.insert("argv".to_string(), serde_json::to_value(cmd_args).unwrap_or(Value::Null));
            params_obj.insert("error".to_string(), Value::String(e.to_string()));

            let _ = ledger.record_action(
                project_name,
                session_id,
                "cli_command",
                &target_bin,
                Value::Object(params_obj),
                &t_start_iso,
                &t_end_iso,
                duration_ms,
                127,
                Some(&format!("Failed to spawn: {}", e)),
                0,
            );
            eprintln!("[blackbox] execution failure: {}", e);
            return Ok(127);
        }
    };

    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();

    let (tx_out, rx_out) = mpsc::channel();
    let (tx_err, rx_err) = mpsc::channel();

    let t_out = thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut snippet_bytes = Vec::new();
        let mut total_bytes = 0i64;
        let mut stdout_handle = io::stdout();

        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 {
                break;
            }
            let _ = stdout_handle.write_all(&buf[..n]);
            let _ = stdout_handle.flush();
            total_bytes += n as i64;
            if snippet_bytes.len() < 4096 {
                let take = std::cmp::min(n, 4096 - snippet_bytes.len());
                snippet_bytes.extend_from_slice(&buf[..take]);
            }
        }
        let _ = tx_out.send((total_bytes, snippet_bytes));
    });

    let t_err = thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut snippet_bytes = Vec::new();
        let mut total_bytes = 0i64;
        let mut stderr_handle = io::stderr();

        while let Ok(n) = stderr.read(&mut buf) {
            if n == 0 {
                break;
            }
            let _ = stderr_handle.write_all(&buf[..n]);
            let _ = stderr_handle.flush();
            total_bytes += n as i64;
            if snippet_bytes.len() < 4096 {
                let take = std::cmp::min(n, 4096 - snippet_bytes.len());
                snippet_bytes.extend_from_slice(&buf[..take]);
            }
        }
        let _ = tx_err.send((total_bytes, snippet_bytes));
    });

    let status = child.wait()?;
    let _ = t_out.join();
    let _ = t_err.join();

    let (bytes_out, chunk_out) = rx_out.recv().unwrap_or((0, Vec::new()));
    let (bytes_err, chunk_err) = rx_err.recv().unwrap_or((0, Vec::new()));

    let t_end_iso = get_iso_timestamp();
    let duration_ms = t_start.elapsed().as_secs_f64() * 1000.0;
    let exit_code = status.code().unwrap_or(1);

    let mut combined_snippet = String::from_utf8_lossy(&chunk_out).to_string();
    if !chunk_err.is_empty() {
        combined_snippet.push_str(&String::from_utf8_lossy(&chunk_err));
    }

    let mut params_obj = serde_json::Map::new();
    params_obj.insert("argv".to_string(), serde_json::to_value(cmd_args).unwrap_or(Value::Null));

    let _ = ledger.record_action(
        project_name,
        session_id,
        "cli_command",
        &target_bin,
        Value::Object(params_obj),
        &t_start_iso,
        &t_end_iso,
        duration_ms,
        exit_code,
        Some(&combined_snippet),
        bytes_out + bytes_err,
    );

    Ok(exit_code)
}

struct PendingMcpCall {
    tool_name: String,
    arguments: Value,
    t_start: Instant,
    t_start_iso: String,
}

pub fn run_mcp_stdio_wiretap(
    server_cmd: &[String],
    project_name: &str,
    session_id: &str,
    db_path: Option<PathBuf>,
) -> io::Result<i32> {
    let ledger = BlackBoxLedger::new(db_path)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    if server_cmd.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "No MCP server command provided"));
    }

    let mut child = Command::new(&server_cmd[0])
        .args(&server_cmd[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    let mut child_in = child.stdin.take().unwrap();
    let child_out = child.stdout.take().unwrap();

    let (tx_inbound, rx_inbound) = mpsc::channel::<String>();
    let (tx_mcp_call, rx_mcp_call) = mpsc::channel::<(Value, String, Value, Instant, String)>();

    // Thread 1: Read from Agent Stdin -> Forward to MCP Server child
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin.lock());
        let mut line = String::new();

        while let Ok(n) = reader.read_line(&mut line) {
            if n == 0 {
                break;
            }
            if let Ok(val) = serde_json::from_str::<Value>(&line) {
                if val.get("method").and_then(|m| m.as_str()) == Some("tools/call") {
                    let id = val.get("id").cloned().unwrap_or(Value::Null);
                    let params = val.get("params").cloned().unwrap_or(Value::Null);
                    let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("unknown").to_string();
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    let _ = tx_mcp_call.send((id, tool_name, arguments, Instant::now(), get_iso_timestamp()));
                }
            }
            let _ = child_in.write_all(line.as_bytes());
            let _ = child_in.flush();
            line.clear();
        }
    });

    // Thread 2: Read from MCP Server child -> Stream to Agent Stdout
    let t_read_out = thread::spawn(move || {
        let mut reader = BufReader::new(child_out);
        let mut line = String::new();
        while let Ok(n) = reader.read_line(&mut line) {
            if n == 0 {
                break;
            }
            let _ = tx_inbound.send(line.clone());
            let mut out = io::stdout();
            let _ = out.write_all(line.as_bytes());
            let _ = out.flush();
            line.clear();
        }
    });

    let mut pending_calls: HashMap<String, PendingMcpCall> = HashMap::new();

    // Process loop: match responses from MCP server
    while let Ok(line) = rx_inbound.recv() {
        // Drain any newly registered tool calls
        while let Ok((id_val, tool_name, args, t_s, t_s_iso)) = rx_mcp_call.try_recv() {
            let id_key = id_val.to_string();
            pending_calls.insert(id_key, PendingMcpCall {
                tool_name,
                arguments: args,
                t_start: t_s,
                t_start_iso: t_s_iso,
            });
        }

        if let Ok(resp_val) = serde_json::from_str::<Value>(&line) {
            if let Some(resp_id) = resp_val.get("id") {
                let id_key = resp_id.to_string();
                if let Some(call_info) = pending_calls.remove(&id_key) {
                    let t_end_iso = get_iso_timestamp();
                    let duration_ms = call_info.t_start.elapsed().as_secs_f64() * 1000.0;
                    let is_error = resp_val.get("error").is_some();
                    let exit_code = if is_error { 1 } else { 0 };
                    let result_str = resp_val.get("result").or_else(|| resp_val.get("error")).map(|v| v.to_string()).unwrap_or_default();

                    let _ = ledger.record_action(
                        project_name,
                        session_id,
                        "mcp_tool_call",
                        &call_info.tool_name,
                        call_info.arguments,
                        &call_info.t_start_iso,
                        &t_end_iso,
                        duration_ms,
                        exit_code,
                        Some(&result_str),
                        result_str.len() as i64,
                    );
                }
            }
        }
    }

    // Drain any remaining tool calls sent right before disconnect
    while let Ok((id_val, tool_name, args, t_s, t_s_iso)) = rx_mcp_call.try_recv() {
        let id_key = id_val.to_string();
        pending_calls.insert(id_key, PendingMcpCall {
            tool_name,
            arguments: args,
            t_start: t_s,
            t_start_iso: t_s_iso,
        });
    }

    // Orphaned MCP Detection: Record unanswered tool calls on disconnect/crash
    for (call_id, call_info) in pending_calls.drain() {
        let t_end_iso = get_iso_timestamp();
        let duration_ms = call_info.t_start.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "[blackbox] WARNING: Orphaned MCP call detected (id={}, tool={}). Server exited before responding.",
            call_id, call_info.tool_name
        );
        let _ = ledger.record_action(
            project_name,
            session_id,
            "mcp_tool_call",
            &call_info.tool_name,
            call_info.arguments,
            &call_info.t_start_iso,
            &t_end_iso,
            duration_ms,
            124, // 124 = Timeout / dropped tool call
            Some("[blackbox] UNANSWERED_MCP_CALL: Server process terminated or disconnected before responding."),
            0,
        );
    }

    let _ = t_read_out.join();
    let status = child.wait()?;
    Ok(status.code().unwrap_or(0))
}

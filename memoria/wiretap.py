import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import List, Optional
from memoria.ledger import MemoriaLedger, get_iso_timestamp

def run_command_wiretap(
    cmd_args: List[str],
    project_name: str = "default",
    session_id: str = "cli_session",
    db_path: Optional[Path] = None
) -> int:
    """
    Execute a CLI command, stream its output directly to stdout/stderr in real-time,
    and automatically record the physical execution to the task ledger.
    """
    ledger = MemoriaLedger(db_path=db_path)
    target_bin = cmd_args[0] if cmd_args else "unknown"
    t_start_iso = get_iso_timestamp()
    t_start = time.perf_counter()

    try:
        proc = subprocess.Popen(
            cmd_args,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1
        )
    except Exception as e:
        t_end = time.perf_counter()
        t_end_iso = get_iso_timestamp()
        duration_ms = (t_end - t_start) * 1000.0
        ledger.record_action(
            project_name=project_name,
            session_id=session_id,
            action_type="cli_command",
            target=target_bin,
            parameters={"argv": cmd_args, "error": str(e)},
            start_time=t_start_iso,
            end_time=t_end_iso,
            duration_ms=duration_ms,
            exit_code=127,
            result_snippet=f"Failed to spawn: {e}",
            bytes_out=0
        )
        print(f"[memoria] execution failure: {e}", file=sys.stderr)
        return 127

    stdout_chunks: List[str] = []
    stderr_chunks: List[str] = []

    # Stream output live so agent and user experience zero buffering lag
    import select
    reads = [proc.stdout, proc.stderr]
    while reads:
        ret = select.select(reads, [], [])
        for fd in ret[0]:
            if fd == proc.stdout:
                line = proc.stdout.readline()
                if line:
                    sys.stdout.write(line)
                    sys.stdout.flush()
                    if len(stdout_chunks) < 50:
                        stdout_chunks.append(line)
                else:
                    reads.remove(proc.stdout)
            elif fd == proc.stderr:
                line = proc.stderr.readline()
                if line:
                    sys.stderr.write(line)
                    sys.stderr.flush()
                    if len(stderr_chunks) < 50:
                        stderr_chunks.append(line)
                else:
                    reads.remove(proc.stderr)

    proc.wait()
    if proc.stdout:
        proc.stdout.close()
    if proc.stderr:
        proc.stderr.close()
    t_end = time.perf_counter()
    t_end_iso = get_iso_timestamp()
    duration_ms = (t_end - t_start) * 1000.0

    snippet = "".join(stdout_chunks[:10] + stderr_chunks[:10])
    total_bytes = sum(len(c.encode("utf-8")) for c in stdout_chunks + stderr_chunks)

    ledger.record_action(
        project_name=project_name,
        session_id=session_id,
        action_type="cli_command",
        target=target_bin,
        parameters={"argv": cmd_args},
        start_time=t_start_iso,
        end_time=t_end_iso,
        duration_ms=duration_ms,
        exit_code=proc.returncode,
        result_snippet=snippet,
        bytes_out=total_bytes
    )
    return proc.returncode


def run_mcp_stdio_wiretap(
    server_cmd: List[str],
    project_name: str = "default",
    session_id: str = "mcp_session",
    db_path: Optional[Path] = None
) -> int:
    """
    Transparent stdio proxy for MCP (Model Context Protocol).
    Sits between the agent harness and the target MCP server:
    - Captures tool calls ('tools/call') and their results.
    - Siphons physical ground truth directly into the task ledger.
    - Forwards all JSON-RPC lines unmodified.
    """
    ledger = MemoriaLedger(db_path=db_path)
    
    server_proc = subprocess.Popen(
        server_cmd,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=sys.stderr,
        text=True,
        bufsize=1
    )

    pending_calls = {}

    import select
    try:
        while True:
            rlist, _, _ = select.select([sys.stdin, server_proc.stdout], [], [])
            
            # Message from Agent -> MCP Server
            if sys.stdin in rlist:
                line = sys.stdin.readline()
                if not line:
                    break
                try:
                    msg = json.loads(line)
                    method = msg.get("method")
                    msg_id = msg.get("id")
                    if method == "tools/call":
                        params = msg.get("params", {})
                        pending_calls[msg_id] = {
                            "tool_name": params.get("name", "unknown"),
                            "arguments": params.get("arguments", {}),
                            "t_start": time.perf_counter(),
                            "t_start_iso": get_iso_timestamp()
                        }
                except Exception:
                    pass

                server_proc.stdin.write(line)
                server_proc.stdin.flush()

            # Message from MCP Server -> Agent
            if server_proc.stdout in rlist:
                line = server_proc.stdout.readline()
                if not line:
                    break
                try:
                    msg = json.loads(line)
                    msg_id = msg.get("id")
                    if msg_id in pending_calls:
                        call_info = pending_calls.pop(msg_id)
                        t_end = time.perf_counter()
                        t_end_iso = get_iso_timestamp()
                        duration_ms = (t_end - call_info["t_start"]) * 1000.0

                        is_error = "error" in msg
                        exit_code = 1 if is_error else 0
                        result_str = json.dumps(msg.get("result", msg.get("error", {})))

                        ledger.record_action(
                            project_name=project_name,
                            session_id=session_id,
                            action_type="mcp_tool_call",
                            target=call_info["tool_name"],
                            parameters=call_info["arguments"],
                            start_time=call_info["t_start_iso"],
                            end_time=t_end_iso,
                            duration_ms=duration_ms,
                            exit_code=exit_code,
                            result_snippet=result_str[:1000],
                            bytes_out=len(result_str.encode("utf-8"))
                        )
                except Exception:
                    pass

                sys.stdout.write(line)
                sys.stdout.flush()

    except KeyboardInterrupt:
        pass
    finally:
        server_proc.terminate()
        server_proc.wait()

    return server_proc.returncode or 0

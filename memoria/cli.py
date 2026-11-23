import argparse
import json
import sys
from pathlib import Path
from memoria.ledger import MemoriaLedger
from memoria.wiretap import run_command_wiretap, run_mcp_stdio_wiretap
from memoria.viewer import generate_html_report

def main():
    parser = argparse.ArgumentParser(
        prog="memoria",
        description="Memoria: The Black Box for Your Black Box — Out-of-band immutable action ledger for AI agents."
    )
    parser.add_argument("--project", "-p", default="default", help="Project name (default: 'default')")
    parser.add_argument("--session", "-s", default="session_0", help="Session ID (default: 'session_0')")
    parser.add_argument("--db", help="Path to Memoria SQLite database (default: ./.memoria/memoria.db)")

    subparsers = parser.add_subparsers(dest="command", help="Subcommand to execute")

    # wrap: CLI Command Wiretap
    wrap_parser = subparsers.add_parser("wrap", help="Wrap and wiretap a CLI command")
    wrap_parser.add_argument("cmd", nargs=argparse.REMAINDER, help="Command to execute (e.g. -- bash -c 'pytest')")

    # mcp: MCP Server Stdio Wiretap
    mcp_parser = subparsers.add_parser("mcp", help="Wrap and wiretap an MCP stdio server")
    mcp_parser.add_argument("cmd", nargs=argparse.REMAINDER, help="MCP server startup command")

    # actions: Query Task Ledger
    actions_parser = subparsers.add_parser("actions", help="Query raw action index (ground reality)")
    actions_parser.add_argument("--limit", "-n", type=int, default=10, help="Number of actions to return")
    actions_parser.add_argument("--json", action="store_true", help="Output raw JSON")

    # context: Query Stage Ledger
    context_parser = subparsers.add_parser("context", help="Query stage milestones and provenance")
    context_parser.add_argument("--limit", "-n", type=int, default=5, help="Number of stages to return")
    context_parser.add_argument("--json", action="store_true", help="Output raw JSON")

    # stage: Advance Stage Milestone
    stage_parser = subparsers.add_parser("stage", help="Record a high-level project stage")
    stage_parser.add_argument("stage_name", help="Name of current stage (e.g. 'planning', 'testing')")
    stage_parser.add_argument("step", help="Description of step completed")
    stage_parser.add_argument("--outcome", "-o", default="success", help="Outcome (success, failure, in_progress)")
    stage_parser.add_argument("--notes", default="", help="Optional notes")

    # verify: Cryptographic Hash Chain Validation
    verify_parser = subparsers.add_parser("verify", help="Verify cryptographic hash chain integrity")

    # export: HTML Flight Report
    export_parser = subparsers.add_parser("export", help="Generate an air-gapped HTML audit report")
    export_parser.add_argument("--out", "-o", help="Output file path (default: ./memoria_audit_<project>.html)")

    args = parser.parse_args()

    db_path = Path(args.db) if args.db else None

    if args.command == "wrap":
        raw_cmd = args.cmd
        if raw_cmd and raw_cmd[0] == "--":
            raw_cmd = raw_cmd[1:]
        if not raw_cmd:
            print("Error: No command specified to wrap. Usage: memoria wrap -- <command> [args...]", file=sys.stderr)
            sys.exit(1)
        code = run_command_wiretap(raw_cmd, project_name=args.project, session_id=args.session, db_path=db_path)
        sys.exit(code)

    elif args.command == "mcp":
        raw_cmd = args.cmd
        if raw_cmd and raw_cmd[0] == "--":
            raw_cmd = raw_cmd[1:]
        if not raw_cmd:
            print("Error: No MCP server command specified. Usage: memoria mcp -- <server_command> [args...]", file=sys.stderr)
            sys.exit(1)
        code = run_mcp_stdio_wiretap(raw_cmd, project_name=args.project, session_id=args.session, db_path=db_path)
        sys.exit(code)

    elif args.command == "actions":
        ledger = MemoriaLedger(db_path=db_path)
        actions = ledger.get_recent_actions(args.project, limit=args.limit)
        if args.json:
            print(json.dumps(actions, indent=2))
        else:
            if not actions:
                print(f"[memoria] No recorded actions found for project '{args.project}'.")
            for a in actions:
                exit_str = "OK" if a.get("exit_code") == 0 else f"ERR({a.get('exit_code')})"
                print(f"[{a['timestamp_start'][:19]}] #{a['id']} {a['action_type']} ➔ {a['target']} ({exit_str}, {a.get('duration_ms', 0):.1f}ms)")
                if a.get("result_snippet"):
                    snippet = a['result_snippet'].strip().replace("\n", " ")
                    if len(snippet) > 80:
                        snippet = snippet[:77] + "..."
                    print(f"   ↳ {snippet}")

    elif args.command == "context":
        ledger = MemoriaLedger(db_path=db_path)
        stages = ledger.get_context(args.project, limit=args.limit)
        if args.json:
            print(json.dumps(stages, indent=2))
        else:
            if not stages:
                print(f"[memoria] No stages recorded yet for project '{args.project}'.")
            for s in stages:
                print(f"[{s['last_updated'][:19]}] #{s['id']} [{s['stage_name'].upper()}] {s['step_description']} (outcome: {s['outcome']})")

    elif args.command == "stage":
        ledger = MemoriaLedger(db_path=db_path)
        sid = ledger.commit_stage(
            project_name=args.project,
            session_id=args.session,
            stage_name=args.stage_name,
            step_description=args.step,
            outcome=args.outcome,
            notes=args.notes
        )
        print(f"[memoria] Stage recorded: id={sid} project='{args.project}' stage='{args.stage_name}'")

    elif args.command == "verify":
        ledger = MemoriaLedger(db_path=db_path)
        is_valid, count, err = ledger.verify_integrity(args.project)
        if is_valid:
            print(f"[memoria] INTEGRITY VERIFIED: {count} blocks in hash chain for '{args.project}'. Zero tampering detected.")
            sys.exit(0)
        else:
            print(f"[memoria] ❌ TAMPERING DETECTED: {err}", file=sys.stderr)
            sys.exit(1)

    elif args.command == "export":
        out = generate_html_report(args.project, db_path=db_path, output_path=Path(args.out) if args.out else None)
        print(f"[memoria] Flight report generated: {out}")

    else:
        parser.print_help()

if __name__ == "__main__":
    main()

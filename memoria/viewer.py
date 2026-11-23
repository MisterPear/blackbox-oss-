import html
import json
from pathlib import Path
from typing import Optional
from memoria.ledger import MemoriaLedger

HTML_TEMPLATE = """<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Memoria Flight Recorder // {project_name}</title>
    <style>
        :root {{
            --bg: #0d1117;
            --surface: #161b22;
            --border: #30363d;
            --text: #c9d1d9;
            --text-dim: #8b949e;
            --accent: #58a6ff;
            --success: #3fb950;
            --danger: #f85149;
            --warning: #d29922;
        }}
        body {{
            background: var(--bg);
            color: var(--text);
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
            margin: 0;
            padding: 2rem;
            line-height: 1.5;
        }}
        .container {{
            max-width: 1100px;
            margin: 0 auto;
        }}
        .header {{
            border-bottom: 1px solid var(--border);
            padding-bottom: 1.5rem;
            margin-bottom: 2rem;
            display: flex;
            justify-content: space-between;
            align-items: center;
        }}
        .tagline {{
            color: var(--text-dim);
            font-size: 0.95rem;
            font-family: monospace;
        }}
        .status-badge {{
            padding: 0.25rem 0.75rem;
            border-radius: 9999px;
            font-size: 0.85rem;
            font-weight: 600;
            font-family: monospace;
        }}
        .badge-verified {{ background: rgba(63, 185, 80, 0.15); color: var(--success); border: 1px solid var(--success); }}
        .badge-tampered {{ background: rgba(248, 81, 73, 0.15); color: var(--danger); border: 1px solid var(--danger); }}
        .card {{
            background: var(--surface);
            border: 1px solid var(--border);
            border-radius: 8px;
            margin-bottom: 1rem;
            overflow: hidden;
        }}
        .card-header {{
            background: rgba(255, 255, 255, 0.02);
            padding: 0.75rem 1rem;
            border-bottom: 1px solid var(--border);
            display: flex;
            justify-content: space-between;
            align-items: center;
            font-size: 0.85rem;
            font-family: monospace;
        }}
        .card-body {{
            padding: 1rem;
        }}
        pre {{
            background: #040d21;
            padding: 0.75rem;
            border-radius: 6px;
            overflow-x: auto;
            font-size: 0.85rem;
            color: #79c0ff;
            margin: 0.5rem 0 0 0;
            border: 1px solid rgba(255, 255, 255, 0.05);
        }}
        .hash {{
            color: var(--text-dim);
            font-family: monospace;
            font-size: 0.75rem;
            word-break: break-all;
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <div>
                <h1 style="margin: 0; font-size: 1.6rem; letter-spacing: -0.5px;">MEMORIA // FLIGHT RECORDER</h1>
                <div class="tagline">The Black Box for Your Black Box • Target: <strong>{project_name}</strong></div>
            </div>
            <div>
                <span class="status-badge {status_class}">{status_text}</span>
            </div>
        </div>

        <h2>Physical Action Ledger ({total_actions} recorded actions)</h2>
        {actions_html}
    </div>
</body>
</html>
"""

def generate_html_report(project_name: str, db_path: Optional[Path] = None, output_path: Optional[Path] = None) -> Path:
    """Generate a self-contained, air-gapped HTML flight recorder report."""
    ledger = MemoriaLedger(db_path=db_path)
    actions = ledger.get_recent_actions(project_name, limit=100)
    is_valid, count, err = ledger.verify_integrity(project_name)

    if is_valid:
        status_class = "badge-verified"
        status_text = f"HASH CHAIN VALID ({count} BLOCKS)"
    else:
        status_class = "badge-tampered"
        status_text = f"TAMPERING DETECTED: {err}"

    cards = []
    for a in actions:
        exit_code = a.get("exit_code", 0)
        code_color = "var(--success)" if exit_code == 0 else "var(--danger)"
        params_str = json.dumps(a.get("parameters", {}), indent=2)
        snippet = a.get("result_snippet", "") or "(no output captured)"

        card_html = f"""
        <div class="card">
            <div class="card-header">
                <div>
                    <strong>#{a['id']} // {html.escape(str(a['action_type']))}</strong> ➔ <code>{html.escape(str(a['target']))}</code>
                </div>
                <div>
                    <span style="color: {code_color}; font-weight: bold;">EXIT {exit_code}</span> • {a.get('duration_ms', 0):.1f}ms • {html.escape(str(a['timestamp_start']))}
                </div>
            </div>
            <div class="card-body">
                <div style="font-size: 0.85rem; color: var(--text-dim); margin-bottom: 0.25rem;">Parameters:</div>
                <pre>{html.escape(params_str)}</pre>
                <div style="font-size: 0.85rem; color: var(--text-dim); margin-top: 0.75rem; margin-bottom: 0.25rem;">Output Snippet:</div>
                <pre style="color: #a5d6ff;">{html.escape(snippet)}</pre>
                <div class="hash" style="margin-top: 0.75rem;">SHA-256 Block: {a.get('entry_hash', '')}</div>
            </div>
        </div>
        """
        cards.append(card_html)

    actions_html = "\n".join(cards) if cards else "<p style='color: var(--text-dim);'>No actions recorded yet.</p>"

    rendered = HTML_TEMPLATE.format(
        project_name=html.escape(project_name),
        status_class=status_class,
        status_text=html.escape(status_text),
        total_actions=len(actions),
        actions_html=actions_html
    )

    if output_path is None:
        output_path = Path.cwd() / f"memoria_audit_{project_name}.html"
    else:
        output_path = Path(output_path)

    output_path.write_text(rendered, encoding="utf-8")
    return output_path

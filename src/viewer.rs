use crate::ledger::MemoriaLedger;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const HTML_HEAD: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Memoria Flight Recorder // __PROJECT__</title>
    <style>
        :root {
            --bg: #0d1117;
            --surface: #161b22;
            --border: #30363d;
            --text: #c9d1d9;
            --text-dim: #8b949e;
            --accent: #58a6ff;
            --success: #3fb950;
            --danger: #f85149;
            --warning: #d29922;
        }
        body {
            background: var(--bg);
            color: var(--text);
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
            margin: 0;
            padding: 2rem;
            line-height: 1.5;
        }
        .container {
            max-width: 1100px;
            margin: 0 auto;
        }
        .header {
            border-bottom: 1px solid var(--border);
            padding-bottom: 1.5rem;
            margin-bottom: 2rem;
            display: flex;
            justify-content: space-between;
            align-items: center;
        }
        .tagline {
            color: var(--text-dim);
            font-size: 0.95rem;
            font-family: monospace;
        }
        .status-badge {
            padding: 0.25rem 0.75rem;
            border-radius: 9999px;
            font-size: 0.85rem;
            font-weight: 600;
            font-family: monospace;
        }
        .badge-verified { background: rgba(63, 185, 80, 0.15); color: var(--success); border: 1px solid var(--success); }
        .badge-tampered { background: rgba(248, 81, 73, 0.15); color: var(--danger); border: 1px solid var(--danger); }
        .card {
            background: var(--surface);
            border: 1px solid var(--border);
            border-radius: 8px;
            margin-bottom: 1rem;
            overflow: hidden;
        }
        .card-header {
            background: rgba(255, 255, 255, 0.02);
            padding: 0.75rem 1rem;
            border-bottom: 1px solid var(--border);
            display: flex;
            justify-content: space-between;
            align-items: center;
            font-size: 0.85rem;
            font-family: monospace;
        }
        .card-body {
            padding: 1rem;
        }
        pre {
            background: #040d21;
            padding: 0.75rem;
            border-radius: 6px;
            overflow-x: auto;
            font-size: 0.85rem;
            color: #79c0ff;
            margin: 0.5rem 0 0 0;
            border: 1px solid rgba(255, 255, 255, 0.05);
        }
        .hash {
            color: var(--text-dim);
            font-family: monospace;
            font-size: 0.75rem;
            word-break: break-all;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <div>
                <h1 style="margin: 0; font-size: 1.6rem; letter-spacing: -0.5px;">MEMORIA // FLIGHT RECORDER</h1>
                <div class="tagline">The Black Box for Your Black Box • Target: <strong>__PROJECT__</strong></div>
            </div>
            <div>
                <span class="status-badge __STATUS_CLASS__">__STATUS_TEXT__</span>
            </div>
        </div>

        <h2>Physical Action Ledger (__TOTAL_ACTIONS__ recorded actions)</h2>
"#;

const HTML_FOOT: &str = r#"
    </div>
</body>
</html>
"#;

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn generate_html_report(
    project_name: &str,
    db_path: Option<PathBuf>,
    output_path: Option<PathBuf>,
) -> io::Result<PathBuf> {
    let ledger = MemoriaLedger::new(db_path)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    let actions = ledger
        .get_recent_actions(project_name, 100)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    let (is_valid, count, err) = ledger
        .verify_integrity(project_name)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    let (status_class, status_text) = if is_valid {
        ("badge-verified", format!("HASH CHAIN VALID ({} BLOCKS)", count))
    } else {
        (
            "badge-tampered",
            format!("TAMPERING DETECTED: {}", err.unwrap_or_default()),
        )
    };

    let mut body_cards = String::new();
    for a in &actions {
        let code_color = if a.exit_code == 0 { "var(--success)" } else { "var(--danger)" };
        let params_pretty = serde_json::to_string_pretty(&a.parameters).unwrap_or_default();
        let snippet = a.result_snippet.as_deref().unwrap_or("(no output captured)");

        body_cards.push_str(&format!(
            r#"
        <div class="card">
            <div class="card-header">
                <div>
                    <strong>#{} // {}</strong> ➔ <code>{}</code>
                </div>
                <div>
                    <span style="color: {}; font-weight: bold;">EXIT {}</span> • {:.1}ms • {}
                </div>
            </div>
            <div class="card-body">
                <div style="font-size: 0.85rem; color: var(--text-dim); margin-bottom: 0.25rem;">Parameters:</div>
                <pre>{}</pre>
                <div style="font-size: 0.85rem; color: var(--text-dim); margin-top: 0.75rem; margin-bottom: 0.25rem;">Output Snippet:</div>
                <pre style="color: #a5d6ff;">{}</pre>
                <div class="hash" style="margin-top: 0.75rem;">SHA-256 Block: {}</div>
            </div>
        </div>
"#,
            a.id,
            escape_html(&a.action_type),
            escape_html(&a.target),
            code_color,
            a.exit_code,
            a.duration_ms,
            escape_html(&a.timestamp_start),
            escape_html(&params_pretty),
            escape_html(snippet),
            a.entry_hash
        ));
    }

    if body_cards.is_empty() {
        body_cards.push_str("<p style='color: var(--text-dim);'>No actions recorded yet.</p>");
    }

    let rendered = HTML_HEAD
        .replace("__PROJECT__", &escape_html(project_name))
        .replace("__STATUS_CLASS__", status_class)
        .replace("__STATUS_TEXT__", &escape_html(&status_text))
        .replace("__TOTAL_ACTIONS__", &actions.len().to_string())
        + &body_cards
        + HTML_FOOT;

    let target_out = match output_path {
        Some(p) => p,
        None => Path::new(&format!("memoria_audit_{}.html", project_name)).to_path_buf(),
    };

    fs::write(&target_out, rendered)?;
    Ok(target_out)
}

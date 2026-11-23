# Memoria: The Black Box for Your Black Box

> **Out-of-band immutable action ledger and execution flight recorder for autonomous AI agents.**

---

## The Problem

Autonomous AI agents suffer from an alignment reflex: **they lie.**

* Agents claim: *"I ran all 42 unit tests and verified the build succeeds,"* when they never touched the compiler.
* Agents hallucinate exit codes, fabricate tool results, and delete execution logs when they make mistakes.
* Existing observability tools (LangSmith, Phoenix) only record what the agent *chooses to report*. If the agent lies, their dashboards blindly mirror the lie.

**Trust is a bug.** You cannot audit intelligence with words; you can only audit intelligence with physical telemetry.

---

## What is Memoria?

Memoria is a **transparent physical wiretap** that sits at the execution boundary between your agent and the operating system.

It intercepts every CLI command, MCP tool call, and system action as it leaves the agent, automatically recording physical execution into an **immutable, cryptographically chained flight ledger**.

The agent has **read-only access** to ground itself in reality, but **zero permissions** to edit, truncate, or rewrite history.

```mermaid
flowchart LR
    Agent["Autonomous Agent<br/>(Claude Code, OpenClaw, LangChain)"]
    Bridge["Memoria Wiretap<br/>(Transparent stdio / process shim)"]
    Tools["Physical Execution<br/>(Bash, MCP Servers, APIs)"]
    Ledger[("Immutable TaskLedger<br/>• SHA-256 Hash Chain<br/>• Raw Exit Codes & Bytes<br/>• Microsecond Timestamps")]

    Agent -->|"1. Triggers Action"| Bridge
    Bridge -->|"2. Forwards Bytes"| Tools
    Bridge -.->|"3. Siphons Ground Truth"| Ledger
    Tools -->|"4. Real Output"| Bridge
    Bridge -->|"5. Returns Output"| Agent
    
    Agent -.->|"Read-only reality check"| Ledger
```

---

## The Two-Part Architecture

Memoria splits agent memory into two decoupled tiers:

1. **`TaskLedger` (The Physical Flight Recorder)**:
   - Microsecond timestamps ($t_{\text{start}}$, $t_{\text{end}}$, duration).
   - Exact binary invoked, raw arguments, exit code, and byte count.
   - **SHA-256 Hash Chain**: Each action is mathematically linked to the previous action ($H_N = \text{SHA256}(H_{N-1} + \text{Payload})$). Any retroactive tampering breaks the chain.

2. **`StageLedger` (Project Milestones & Provenance)**:
   - High-level stage progression (e.g. `planning`, `testing`, `deploying`).
   - Sequence provenance without context window bloat.

---

## 30-Second Quickstart

### 1. Installation (Zero External Dependencies)

Memoria is built entirely with Python standard library. No external pip bloat.

```bash
git clone https://github.com/mallard/memoria.git
cd memoria
pip install -e .
```

### 2. Wrap Any CLI Tool
Prefix any command with `memoria wrap`:
```bash
memoria wrap -- pytest tests/
memoria wrap -- cargo build --release
```

### 3. Wiretap Any MCP Server (Model Context Protocol)
Wrap your MCP server in your `claude_desktop_config.json` or agent config:
```json
{
  "mcpServers": {
    "my_tool": {
      "command": "memoria",
      "args": ["mcp", "--", "python3", "/path/to/server.py"]
    }
  }
}
```

### 4. Grounding Reality
When you (or the agent) want to verify what was actually executed:
```bash
# Query the physical action index
memoria actions --limit 5

# Query project milestone context
memoria context

# Verify cryptographic hash chain integrity
memoria verify
```

### 5. Export Standalone Air-Gapped HTML Report
Generate an interactive, standalone HTML audit report:
```bash
memoria export --out flight_report.html
```

---

## The Agent Contract (`MEMORIA.md`)

Drop `MEMORIA.md` into your repository. It instructs OpenClaw, Claude Code, Cursor, or any agent framework:
1. That its execution is being recorded at the wire.
2. That it cannot delete or forge logs.
3. How to run `memoria actions` to ground its answers in physical reality before claiming a task is complete.

---

## License

MIT License. Designed and built by Mallard.

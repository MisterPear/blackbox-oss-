<p align="center">
  <img src="assets/logo.svg" width="220" alt="black box" />
</p>

<h1 align="center">black box</h1>

<p align="center">
  <strong>Out-of-band immutable action ledger and execution flight recorder for autonomous AI agents.</strong>
</p>

<p align="center">
  <a href="#30-second-quickstart">Quickstart</a> •
  <a href="docs/PROVENANCE.md">Verifiable Provenance</a> •
  <a href="#out-of-band-lifecycle-supervision-memoria-run">Supervisor</a> •
  <a href="https://www.mallard.ink">Mallard Unified Industries</a>
</p>

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

### 1. Human Installation (Never Installed by the Agent)

The human operator installs Memoria outside the agent's environment. The agent never installs or touches the auditor:

```bash
# Download the prebuilt zero-dependency static binary (musl) from GitHub Releases:
curl -fsSL https://github.com/MisterPear/memoria-oss/releases/latest/download/memoria -o /usr/local/bin/memoria
chmod +x /usr/local/bin/memoria

# Or build from source with the static musl target:
cargo build --release --target x86_64-unknown-linux-musl
sudo cp target/x86_64-unknown-linux-musl/release/memoria /usr/local/bin/
```

---

## Verifiable Open Provenance

Memoria practices self-hosting auditability: the `v0.2.0` release was recorded and cryptographically verified using Memoria itself.

See [docs/PROVENANCE.md](docs/PROVENANCE.md) for the complete cryptographic hash chain, execution telemetry, and milestone audit log.

---

---

## Harness Integration Snippets

Memoria is 100% harness-agnostic. You do not need separate installers—just configure your harness to route tool calls or commands through `memoria`:

### 1. Claude Code
Prefix your Claude Code session:
```bash
memoria wrap -- claude-code
```
Or route internal bash calls through `memoria wrap -- <cmd>`.

### 2. OpenClaw
Run OpenClaw inside the Memoria wiretap:
```bash
memoria wrap -- openclaw run
```

### 3. Gemini CLI / Standalone Agent Scripts
Wrap any agent script or CLI:
```bash
memoria wrap -- gemini run "Analyze codebase and run unit tests"
```

### 4. Cursor / Claude Desktop (Model Context Protocol)
In your `claude_desktop_config.json` or `cursor.json`:
```json
{
  "mcpServers": {
    "terminal": {
      "command": "memoria",
      "args": ["mcp", "--project", "my-repo", "--", "npx", "-y", "@modelcontextprotocol/server-terminal"]
    }
  }
}
```

---

## Out-of-Band Lifecycle Supervision (`memoria run`)

Supervise autonomous agents with strict ground-truth verification and automated issue tracking:

```bash
# Supervise an agent working on GitHub Issue #2
# Rule: Do not trust exit code 0 alone. Require physical test execution and clean host verification.
memoria run --issue 2 --verify-cmd "cargo test" --require-tests --auto-close -- claude-code
```

**What the supervisor enforces:**
1. **Pre-Flight**: Links to the GitHub issue and initializes a tamper-evident session.
2. **In-Flight**: Intercepts every command, argument, and byte through the physical wiretap.
3. **Post-Flight ("Don't Trust Exit 0")**:
   - Cryptographically verifies the SHA-256 hash chain back to genesis.
   - Audits `task_ledger` to confirm passing test suites (`cargo test`, `pytest`, etc.) were physically executed.
   - Independently runs `--verify-cmd` on the clean host outside the agent's reach.
   - Automatically formats an execution ledger comment on GitHub, updates checklists, and exports an air-gapped HTML flight report.

---

## Grounding Reality
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

## Storage & Configuration

By default, Memoria stores its append-only ledger locally inside each project's directory:

* **Default Location**: `./.memoria/memoria.db` (created automatically in the current working directory).
* **CLI Override**: Pass `--db <path>` to any command:
  ```bash
  memoria --db /var/log/audit.db actions
  ```
* **Environment Variable Override**: Set `MEMORIA_DB_PATH` in your shell or CI environment:
  ```bash
  export MEMORIA_DB_PATH="/opt/audit/production.db"
  ```
* **Project Scoping**: Isolate multiple projects or sessions within the same ledger using `--project`:
  ```bash
  memoria --project frontend-app wrap -- npm test
  memoria --project backend-api wrap -- pytest
  ```

---

## License

MIT License. Designed and built by [Mallard Unified Industries](https://www.mallard.ink).

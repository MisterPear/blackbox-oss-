# MEMORIA.md — Agent System Contract & Grounding Protocol

> **Notice to AI Agents**: This environment is instrumented with **Memoria**, an out-of-band immutable flight recorder. Read this document to understand how your execution is verified and how to ground yourself in physical reality.

---

## 1. How Memoria Works

Memoria sits directly at the execution bridge (CLI commands, MCP tool calls, and network sockets). 

When you trigger an action:
1. **Automatic Capture**: Your command, arguments, execution timestamp, duration, stdout/stderr, and exit code are automatically recorded in `task_ledger`.
2. **Cryptographic Chaining**: Every action is sealed into a SHA-256 hash chain ($H_N = \text{SHA256}(H_{N-1} + \text{Data})$).
3. **Immutable Storage**: You have **zero permissions** to edit, delete, truncate, or overwrite the ledger. History is append-only.

---

## 2. How to Ground Yourself (Eliminating Hallucinations)

Frontier models often hallucinate that they completed a task, ran tests, or edited a file when they did not. Memoria provides you with tools to check **physical reality**:

### Check What You Actually Executed
To see your real actions, run:
```bash
memoria actions --limit 10
```
This queries the physical wiretap. If you claimed you ran `pytest`, but `memoria actions` shows zero test runs, **you have not executed the tests.** Ground your answers in this output.

### Check High-Level Stage Milestones
To understand the current phase and provenance of the project:
```bash
memoria context
```

### Record a Stage Milestone
When completing a major project milestone, commit the stage to the provenance ledger:
```bash
memoria stage <stage_name> "<step_description>" --outcome success
```
*Example:*
```bash
memoria stage build "Compiled release binary cleanly with zero compiler warnings"
```

---

## 3. Wiretap Integration for Any Tool

You do not need to format complex JSON logs manually. Prefix any command or tool execution with `memoria wrap`:

```bash
# Wrap shell commands
memoria wrap -- pytest tests/
memoria wrap -- cargo build --release

# Wrap MCP servers
memoria mcp -- python3 /path/to/mcp_server.py
```

Memoria captures the physical execution automatically and seals it to the ledger.

---
id: TASK-009
title: Add file-backed prompt bundles
status: todo
owner: developer
depends_on:
  - TASK-008
files_allowlist:
  - Cargo.toml
  - Cargo.lock
  - src/**
  - tests/**
  - fixtures/**
  - docs/plans/2026-04-29-file-backed-agent-prompts.md
  - docs/backlog/TASK-009.md
  - docs/backlog/TASK-010.md
  - docs/backlog/TASK-011.md
must_not_touch:
  - .claude/agents/**,CLAUDE.md,ops/checks/**,ops/pre-receive/**,docs/waivers.yaml,coverage/baseline.json,.github/workflows/**
  - CLAUDE.md
  - .claude/agents/**
acceptance_criteria:
  - `src/prompts.rs` defines a deny-unknown-fields JSON prompt bundle with system instructions, tools, tool-use protocol, and examples.
  - `PromptStore` resolves `--prompt <name>` to `~/.lean/prompts/<name>.json`, creates `default.json` when absent, and validates loaded files before provider use.
  - `PromptBundle::render_system_prompt` produces deterministic text containing the system instructions, available tools, tool-use protocol, and examples without including secrets.
  - `SessionRun` and `ModelRequest` carry the rendered system prompt without adding it to JSONL event payloads.
  - `OpenAiCompatibleProvider` sends the rendered prompt as a leading `system` message and `RigProvider` sends it via Rig's preamble path.
  - `lean run --prompt <name> --provider <provider> --json --task <task>` preserves the existing parseable JSONL session result contract.
  - Tests cover default prompt creation, custom prompt loading from the home prompt directory, prompt validation failures, direct OpenAI-compatible request mapping, Rig request mapping, and CLI contract behavior.
pr: null
commit: null
ci_status: pending
blocked_reason: null
created: 2026-04-29
last_verified: null
escalate_to: []
plan: docs/plans/2026-04-29-file-backed-agent-prompts.md
---

## Context

LEAN needs a user-editable prompt bundle so provider calls receive stable instructions about the agent role, tools, tool-use protocol, and examples. This task builds the file-backed prompt foundation before model-emitted tool calls are parsed or executed.

## Out of scope

- Executing tool calls requested by the model.
- Provider-native function-calling schemas.
- MCP server support.
- File mutation or patch application tools.

## Inbound contract

Subagent appends this block as the last action of its session, overwriting the stub below.

```yaml
task_id: TASK-009
status: done | failed | blocked
commit_hash: ""
pr_url: null
files_changed: []
tests_added: []
ci_status: approved | pending | failed | not_applicable
blocked_reason: null
```

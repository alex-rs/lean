---
id: TASK-010
title: Parse model tool-use requests
status: todo
owner: developer
depends_on:
  - TASK-009
files_allowlist:
  - src/**
  - tests/**
  - fixtures/**
must_not_touch:
  - .claude/agents/**,CLAUDE.md,ops/checks/**,ops/pre-receive/**,docs/waivers.yaml,coverage/baseline.json,.github/workflows/**
  - CLAUDE.md
  - .claude/agents/**
acceptance_criteria:
  - `ToolUseRequest` parses the JSON protocol described by the default prompt bundle and rejects unknown or malformed tool-use payloads with structured errors.
  - The parser accepts exactly one tool-use request per assistant turn and leaves normal final-answer text unchanged.
  - Tool-use parse errors are sanitized and do not include raw prompt files, secrets, or raw provider response bodies.
  - Tests cover valid `read_file`, valid `list_directory`, unknown tool, invalid arguments, multiple requests, and normal final-answer parsing.
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

After prompt bundles describe LEAN's tool protocol, the runtime needs to recognize when a model is asking for a tool instead of returning a final answer. This task adds parsing only; execution remains isolated in the follow-up task.

## Out of scope

- Executing any parsed tool request.
- Adding write tools or command execution.
- Provider-native function-calling APIs.

## Inbound contract

Subagent appends this block as the last action of its session, overwriting the stub below.

```yaml
task_id: TASK-010
status: done | failed | blocked
commit_hash: ""
pr_url: null
files_changed: []
tests_added: []
ci_status: approved | pending | failed | not_applicable
blocked_reason: null
```

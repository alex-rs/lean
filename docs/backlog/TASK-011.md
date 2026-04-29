---
id: TASK-011
title: Execute read-only tool requests in the session loop
status: todo
owner: developer
depends_on:
  - TASK-010
files_allowlist:
  - src/**
  - tests/**
  - fixtures/**
must_not_touch:
  - .claude/agents/**,CLAUDE.md,ops/checks/**,ops/pre-receive/**,docs/waivers.yaml,coverage/baseline.json,.github/workflows/**
  - CLAUDE.md
  - .claude/agents/**
acceptance_criteria:
  - `SessionRunner` executes parsed `read_file` and `list_directory` requests through `ReadTools` only inside the session workspace.
  - Tool results are returned to the provider in a follow-up turn using the protocol from the prompt bundle.
  - The run loop enforces `runtime.max_turns` for tool-call cycles and emits a sanitized session error when the limit is reached.
  - Tests cover successful read-file and list-directory cycles, workspace escape rejection, unknown paths, and max-turn exhaustion.
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

Once LEAN can parse model tool-use requests, it can safely wire those requests to the existing read-only workspace tools. This task keeps execution read-only so mutation and validation can remain separate future work.

## Out of scope

- File mutation, patch application, or command execution tools.
- Parallel tool calls.
- Provider-native function calling.

## Inbound contract

Subagent appends this block as the last action of its session, overwriting the stub below.

```yaml
task_id: TASK-011
status: done | failed | blocked
commit_hash: ""
pr_url: null
files_changed: []
tests_added: []
ci_status: approved | pending | failed | not_applicable
blocked_reason: null
```

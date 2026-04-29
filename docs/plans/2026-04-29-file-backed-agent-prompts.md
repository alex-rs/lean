---
slug: file-backed-agent-prompts
status: draft
created: 2026-04-29
founder_approved: 2026-04-29
founder_approved_by: founder
tasks:
  - TASK-009
  - TASK-010
  - TASK-011
supersedes: null
---

# File-Backed Agent Prompts

## Goal

Teach LEAN's model providers how to receive a durable agent instruction bundle: system prompt, available tools, tool-use protocol, and examples. The bundle must be JSON on disk under the current user's `~/.lean/prompts/` directory so operators can update prompt behavior without recompiling LEAN.

## Non-goals

This phase does not execute model-emitted tool calls, add MCP servers, add provider-native function-calling schemas, or implement write tools. It also does not move repo governance instructions into prompt JSON.

## Constraints

Prompt files are user-editable runtime inputs, so they need strict parsing and validation before they reach a provider. LEAN must not log prompt bundle contents, raw provider request bodies, API keys, or raw provider responses. The implementation should keep the provider boundary generic: direct OpenAI-compatible providers can send a `system` message, while Rig-backed providers can use Rig's preamble/system-message path.

References:
- https://opencode.ai/docs/tools
- https://code.claude.com/docs/en/settings
- https://code.claude.com/docs/en/agent-sdk/modifying-system-prompts

## Approach

First, add a JSON prompt bundle schema and loader. `lean run` should resolve `--prompt default` to `~/.lean/prompts/default.json`, create an editable default bundle when that file is absent, and validate every loaded bundle before use.

Second, render the loaded bundle into a provider-neutral system prompt. The rendered prompt should include the system instructions, current tool list, tool-use response protocol, and examples in a deterministic order.

Third, carry that rendered prompt through the existing session/provider request boundary. OpenAI-compatible requests should prepend a system chat message; Rig-backed requests should set the Rig preamble so supported provider families receive equivalent instructions.

Fourth, keep actual tool execution as follow-up work. Once the prompt is in place, TASK-010 and TASK-011 can add parsing and read-only tool execution without changing the prompt storage contract.

## Task Breakdown

1. TASK-009 - Add file-backed prompt bundles - owner: developer.
2. TASK-010 - Parse model tool-use requests - owner: developer.
3. TASK-011 - Execute read-only tool requests in the session loop - owner: developer.

## Risks

The main risk is treating editable prompt JSON as trusted input. The mitigation is deny-unknown-fields parsing plus validation for required text, unique tool names, and object-shaped input schemas.

A second risk is accidentally leaking large prompt contents into JSONL events or error strings. The mitigation is to keep prompt contents out of event payloads and expose only sanitized parse/load errors.

## Acceptance

This phase is accepted when `TASK-009` is merged and LEAN can load an editable prompt JSON from `~/.lean/prompts/`, inject the rendered system prompt into real provider requests, and preserve the existing JSONL session contract. `TASK-010` and `TASK-011` remain the follow-up path for actual tool-call parsing and execution.

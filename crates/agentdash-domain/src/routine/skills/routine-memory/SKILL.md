---
name: routine-memory
description: AgentDashboard Routine memory protocol. Use when a session is started by a scheduled, webhook, or plugin-triggered Routine and has a routine:// VFS mount; read trigger facts and durable Routine/entity memory for cross-trigger continuity, then update only concise future-use facts, decisions, open items, or entity outcomes.
---

# Routine Memory

Use this skill when the current session was started by a Routine and a `routine://` VFS mount is available.

Routine memory is a compact, durable working memory for the automation rule. It is not a transcript dump. Use it only when the current task benefits from cross-trigger continuity.

For the full file semantics, read `references/memory-model.md`.

## Read Order

1. Read `routine://memory/brief.md` to understand the Routine's durable purpose.
2. Read `routine://memory/open-items.md` when the task may continue prior work.
3. If `routine://current/execution.json` has an `entity_key`, read `routine://entities/{entity_key}/brief.md` and `routine://entities/{entity_key}/open-items.md`.
4. Read `routine://current/trigger.json` for this trigger's payload and source facts.

## Update Rules

- Update `memory/facts.md` only with durable facts that should survive future triggers.
- Update `memory/decisions.md` only with decisions that affect future behavior.
- Update `memory/open-items.md` with unresolved follow-up work.
- Update `entities/{entity_key}/last-run.md` after entity-scoped work when the outcome matters for the next trigger.
- Keep entries concise. Prefer summaries, facts, decisions, and recovery points over raw logs.

## Boundaries

- Treat `current/*` as read-only trigger facts.
- Do not copy large payloads, transcripts, or tool logs into memory.
- Do not invent facts. Mark uncertain observations as pending confirmation.
- Use the Routine prompt as the task instruction and Routine memory as supporting context.

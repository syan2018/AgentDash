# Dispatch Tracking

## Current Trellis State

- Active task: `.trellis/tasks/07-06-agent-lifecycle-fact-source-review`
- Status: `in_progress`
- Branch: `codex/agent-lifecycle-fact-source-review`
- Planning baseline commit: `ce74bea5 chore(trellis): 启动 Agent 生命周期边界维护任务`
- Collaboration channel: `agent-lifecycle-boundary`
- Channel path: `C:\Users\Syan\.trellis\channels\F--Projects-AgentDash\agent-lifecycle-boundary`

## Workflow Recovery

If context is compacted, restore the working state in this order:

1. Run `python ./.trellis/scripts/task.py current --source` and confirm the active task above.
2. Re-read `prd.md`, `design.md`, `implement.md`, `implement.jsonl`, `check.jsonl`, and this file.
3. Inspect the channel with `trellis channel messages agent-lifecycle-boundary --raw --last 80`.
4. Check worker completion with `trellis channel wait agent-lifecycle-boundary --as codex-main --from "impl-wait-gate,impl-control-effects,impl-protocol-frontend" --kind "done,error" --all --timeout 1m`.
5. Review `git status --short` before touching files. Do not overwrite worker changes.
6. Commit each completed work package independently.

## Active Workers

### `impl-wait-gate`

- Scope: WP2 Wait/Gate Typed Payload.
- Owns: typed `LifecycleGate.payload_json` wait policy envelope, generic producer terminal convergence naming, repository lookup path semantics.
- Avoids: AgentRun control-effect outbox, RuntimeSession effect migration, frontend refresh/protocol work.
- Latest observed progress: reading workflow convergence, payload write, SQL JSON path, and related domain/application files.

### `impl-control-effects`

- Scope: WP3 Session Residue Excision and WP4 AgentRun Control-Plane Effects.
- Owns: moving `hook_effects`, `hook_auto_resume`, and `session_terminal_callback` replay away from RuntimeSession naming/ownership into AgentRun control-effect boundaries.
- Avoids: wait/gate typed envelope and frontend refresh mapping unless required for compile.
- Latest observed progress: found old outbox spans SPI, runtime-session, infrastructure, and API bootstrap; prioritizing SPI/infrastructure naming and record model migration toward `AgentRunControlEffect*`.

### `impl-protocol-frontend`

- Scope: WP5 Projection Invalidation Event and WP6 Frontend Boundary.
- Owns: `ControlPlaneProjectionChanged`, generated TS protocol path, `controlPlaneModel` refresh planning, terminal store stream-scoped dedup.
- Avoids: AgentRun control-effect outbox and wait/gate envelope changes.
- Latest observed progress: reading Backbone platform protocol, generated protocol, `controlPlaneModel`, terminal dispatcher, and terminal store.

## Commit Slicing

- Commit planning/task tracking separately from implementation.
- Commit WP2 wait/gate changes independently.
- Commit WP3/WP4 Session residue and AgentRun control-effect changes independently; split again if migration/model and executor rewiring are separable.
- Commit WP5/WP6 protocol/frontend changes independently after contract generation and focused frontend tests.
- Run check agent after each coherent implementation slice before committing the slice.

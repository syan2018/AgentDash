# Autonomy Protocol

## Purpose

This file is the recovery and execution contract for continuing the architecture convergence work while the user is away.

If context compaction happens, the next assistant must read this file before taking action. The first recovered action must be to restore Trellis workflow state, git state, active goal, active task, and any in-flight subagent status before continuing.

## Current Long-Term Goal

Proceed through the architecture convergence sequence:

1. Finish and preserve the module adversarial review artifacts.
2. Execute `.trellis/tasks/06-30-architecture-quick-convergence/` by dispatching subagents over the five `work-items/`.
3. Commit the quick convergence implementation as a coherent set of module commits, not as tiny tracking commits.
4. Create and run a follow-up Trellis task for the full Design backlog review.
5. For each Design item:
   - self-complete research and implementation design when no serious product/architecture choice is required;
   - when a serious decision is required, document the decision point, alternatives, trade-offs, and recommended answer for the user.

## Goal Execution Addendum

Treat this section as the durable addendum to the active Codex goal. The in-app goal object records the high-level objective; this file records the operational process that must be restored after context compaction.

The execution model is:

1. The main session owns Trellis state, task status, branch hygiene, subagent dispatch, synthesis, spec updates, commits, and final finish-work.
2. Implementation and check work should default to Trellis subagents when available:
   - use `trellis-implement` for scoped implementation;
   - use `trellis-check` for post-change review and verification;
   - use `trellis-research` for design backlog investigation when isolated code archaeology or option analysis can run independently.
3. Subagents are a parallel execution mechanism, not a coordination ceremony. Parallel review and redundant findings are acceptable; the main session synthesizes conflicts and overlap after workers report back.
4. Every subagent dispatch prompt starts with `Active task: <task path from task.py current>`, then instructs the agent to read jsonl context, `prd.md`, `design.md` when present, `implement.md` when present, and the assigned work-item or design backlog section. Every dispatch must also repeat the cleanup-first constraint: this review exists to converge first-principles architecture, so removing wrong paths, duplicated facts, and concept forks is more important than adding new feature surface.
5. Work items under `.trellis/tasks/06-30-architecture-quick-convergence/work-items/` are implementation briefs inside one parent task. They are not Trellis child tasks, because the desired unit of management is one convergence task with independently assignable work items.
6. File-overlap is handled pragmatically:
   - when scopes are independent, dispatch in parallel;
   - when scopes overlap, let the higher-priority or smaller invariant-setting item land first;
   - after subagents finish, inspect the combined diff rather than assuming each worker's patch is final.
7. Commit boundaries follow durable review value:
   - group quick convergence implementation by coherent module area or by one validated combined slice;
   - group design backlog review as a complete review package;
   - group later implementation follow-ups by meaningful behavior changes.
8. Task metadata, status notes, and single-file tracking edits are accumulated into nearby meaningful commits instead of becoming standalone commits.
9. User involvement is reserved for serious design/product choices. When the codebase and specs imply a clear answer, document and proceed. When there are multiple viable long-term owners or public contract shapes, record the decision options and recommendation for the user.
10. Subagent work must prefer first-principles cleanup over additive feature work. The review objective is convergence: remove wrong paths, duplicated facts, and concept forks when the scope is acceptable; do not satisfy a work item by layering another abstraction on top of the old split unless the old path is also removed or explicitly documented as a larger design residual. A worker that only adds a new feature-shaped path while leaving the old split intact has not completed the assignment.
11. Implementation subagents must not run large Rust builds or broad compile suites on their own. They may run narrow searches, formatting, small focused tests when cheap, or inspect existing test names. Expensive Rust compilation and broad verification belong to the check/integration phase unless a worker needs one targeted command to validate a small local change.
12. While subagents are running, the main session should not do anxious overlapping implementation or repeatedly interrupt them. It may wait, do non-overlapping coordination/documentation, or send at most one concise status/addendum message when requirements change or a worker appears stuck.
13. Context compaction recovery must restore, in this order: active goal, Trellis current task and source, workflow phase, branch, worktree status, recent commits, in-flight subagent state, and the next executable step.

## Recovery Checklist After Context Compaction

Run these commands first:

```powershell
python ./.trellis/scripts/get_context.py
python ./.trellis/scripts/task.py current --source
git branch --show-current
git status --short
git log -5 --oneline
```

Then inspect:

```powershell
Get-Content -LiteralPath '.trellis\tasks\06-30-module-adversarial-review\followups\autonomy-protocol.md' -Raw
Get-Content -LiteralPath '.trellis\tasks\06-30-architecture-quick-convergence\implement.md' -Raw
rg --files .trellis/tasks/06-30-architecture-quick-convergence/work-items
```

If the current task is not the task you are about to work on, switch explicitly with:

```powershell
python ./.trellis/scripts/task.py start <task-dir>
```

Never infer phase from memory alone. Use task status and artifacts.

## Branch And Commit State

The review/planning artifacts are on:

- Branch: `codex/module-adversarial-review-cleanup`
- Planning commit at the time this protocol addendum was written: `5540671f docs(trellis): 记录模块对抗审查与快速收束规划`

Continue work on this branch unless the user explicitly asks otherwise.

Before implementation, check whether the branch has uncommitted changes. Do not overwrite unrelated user/parallel work.

## Trellis Workflow Rules

Always follow Trellis:

1. Planning artifacts before implementation.
2. `task.py start` before Phase 2 implementation.
3. Dispatch implement/check subagents from the main session.
4. Run relevant checks.
5. Update specs only if implementation teaches a reusable rule.
6. Commit work in meaningful batches.
7. Finish/archive only after commits and journal steps are appropriate.

Ordinary work items stay as files under the parent task. This keeps the review and implementation evidence in one task while still allowing independent worker assignment.

## Subagent Dispatch Rules

Main session owns coordination. Use native subagents, not Trellis channel, unless the user explicitly requests channel.

Use `trellis-implement` for implementation and `trellis-check` for verification when available.

Each dispatch prompt must start with:

```text
Active task: <task path from task.py current>
```

Each implement subagent must:

- read `implement.jsonl`;
- read `prd.md`;
- read `design.md`;
- read `implement.md`;
- read the assigned `work-items/<name>.md`;
- modify only its assigned work item scope;
- prioritize deleting or converging wrong/duplicate paths over adding parallel new feature paths;
- avoid large Rust compilation or broad check commands; leave expensive verification for check/integration unless a narrow local command is necessary;
- treat cleanup of old architectural mistakes as higher priority than feature completion when the two conflict inside the assigned scope;
- not revert unrelated changes;
- report files changed and checks run.

Each check subagent must:

- verify that the implementation actually removed or converged the old split instead of adding a parallel path;
- keep verification targeted to the touched crates/modules unless the main session explicitly asks for broader integration checks;
- report any large or risky check it intentionally skipped so the main session can decide whether to run it.

Parallelism strategy:

- It is acceptable and expected to run work items in parallel.
- Overlap must be controlled by file scope, not by avoiding review overlap.
- If two work items touch the same file, dispatch the higher-priority item first and defer the overlapping part of the other item.

## Quick Convergence Work Items

Parent task:

```text
.trellis/tasks/06-30-architecture-quick-convergence/
```

Work items:

1. `work-items/01-authority-capability-admission.md`
   - Priority: first.
   - Handles two P0 issues.
   - Do not mix with full AgentRunEffectiveCapabilityPort design.
2. `work-items/02-extension-workspace-module-consistency.md`
   - Schema validator, invocation workspace resolver, renderer-aware loadability.
3. `work-items/03-vfs-local-guard-rails.md`
   - Tool name guard, workspace root guard, handler-declared scheduling, builtin skill identity.
4. `work-items/04-mailbox-steering-consistency.md`
   - Steering executor and status/receipt semantics.
5. `work-items/05-settings-preference-convergence.md`
   - Scoped settings migration from legacy `user_preferences`.

Suggested execution:

- Start Authority first.
- Extension and VFS/Local can run in parallel if file overlap is acceptable.
- Mailbox can run independently.
- Settings should be isolated because it requires migration.

## Design Backlog Review

Design backlog source:

```text
.trellis/tasks/06-30-module-adversarial-review/followups/design-backlog.md
```

After quick convergence implementation is committed, create a new Trellis task for the full design backlog review.

The design review task must:

- include all D1-D12 items;
- proceed from simplest to most complex;
- use research subagents where useful;
- write one design review document per major area, or one consolidated document with clear sections;
- mark decision points as one of:
  - self-decided implementation design;
  - user decision required;
  - blocked by post-quick-convergence code shape.

For user decision required items, document:

- exact decision;
- why it matters;
- available options;
- recommended option;
- trade-offs;
- code evidence;
- what implementation becomes possible after the decision.

Do not stall waiting for user on decisions that can be made from code/spec evidence.

## Commit Policy

Do not make tiny commits for task start, single tracking updates, or one document status tick.

Use coherent commit batches:

1. Review/planning artifacts: already committed as `2cf25c8e`.
2. Quick convergence implementation:
   - Prefer one commit per coherent module group if changes are large:
     - Authority/capability;
     - Extension/workspace-module;
     - VFS/local;
     - Mailbox;
     - Settings migration.
   - If changes are small and checks pass together, one combined commit is acceptable.
3. Design backlog review:
   - One documentation commit for the complete design review package.
4. Subsequent implementation tasks:
   - One commit per meaningful implementation slice, not per task metadata update.

Commit message format:

```text
type(scope): 可保留英文专业用词的中文提交信息

- 分点描述具体更新内容
- 包含验证结果或重要未运行说明
```

## User-Decision Boundary

Self-decide:

- local refactors with clear single owner;
- replacing duplicate helpers with shared helper;
- aligning implementation with existing spec;
- adding missing guards/invariants;
- moving old preference reads to existing scoped settings if key semantics are clear.

Document for user decision:

- choosing between two valid long-term owners;
- removing product-visible capability or changing workflow semantics;
- introducing a new public contract shape with multiple plausible designs;
- changing whether a domain owns discovery/catalog/invocation;
- any design that would block another team/agent without a clear code/spec answer.

## Validation Discipline

For implementation:

- run targeted Rust tests/checks for touched crates;
- run frontend typecheck/tests if frontend touched;
- run `pnpm run migration:guard` if migration touched;
- do not run broad full-suite repeatedly for small changes unless risk demands it.

For docs/review-only:

- `git status --short`;
- inspect file paths and links;
- no full test required.

## Operating Invariants

- Native subagents are the active execution surface for this goal because the user requested fast direct dispatch and the channel runtime was not the desired path.
- A task is ready for implementation only after the relevant Trellis artifacts and jsonl context give workers enough module-specific guidance to edit without rediscovering the whole project.
- Active tasks remain open until their required artifacts, implementation or review outputs, validation notes, and meaningful commits are complete.
- Long-running shell sessions are treated as owned resources; finish, stop, or report them before ending a turn.
- Existing uncommitted changes are assumed to belong to the user or parallel sessions. Work with them when relevant and leave unrelated changes untouched.
- The project is pre-release, so implementation should converge on the correct current model instead of preserving compatibility paths that only encode abandoned shapes.

# Research Brief: AgentDashboard current wait / exec / companion chain

Active task: `.trellis/tasks/07-03-waitable-activity-exec-closure`

You are a read-only research worker. Do not edit files, do not commit, do not push.

## Objective

Inspect the current AgentDashboard codebase and map the existing execution, tool, companion, mailbox, lifecycle gate, ContextFrame, and frontend projection chain relevant to a unified wait module.

The user requirement is not "add a UI waiting row" and not "each tool implements its own wait". The target is a common wait module used by all tools that can return waiting behavior:

- exec long-running commands and interactive shell/process sessions.
- companion/subagent dispatch and result return.
- human/user response waits.
- mailbox/wake activity.
- future runtime activity sources.

## Questions To Answer

1. Where is Agent runtime tool catalog assembled? How does a tool become visible to the Agent?
2. Where is `main://` / VFS `exec` implemented? What does it return for `state: running` and `session_id`?
3. Is there any existing read/wait/input/terminate/status path for that `session_id` available to Agent tools? If yes, where; if no, where is the break?
4. How do local shell/process/terminal sessions store output, status, and exit code?
5. How do companion/subagent/human waits currently work? Where does `wait=true` poll? Where are gates created/resolved?
6. How does AgentRun mailbox store, dedupe, deliver, and notify result envelopes?
7. Where should a common wait module live by existing architecture boundaries: domain/application/runtime gateway/local backend/AgentRun mailbox?
8. What frontend projection already exists and what is missing?
9. What ContextFrame / Environment path should declare Windows PowerShell shell semantics?
10. What tests already exist and what new tests are required?

## Constraints

- No new external `/sessions/*` endpoint.
- RuntimeSession remains a runtime trace/delivery ref, not workspace command owner.
- Scheduler/mailbox remain delivery authority.
- Result bodies should stay in bounded buffers/mailbox/artifacts/projections; wait returns refs and summaries.
- Companion and human/user wait must migrate into the new wait path instead of remaining private tool polling.

## Output

Write a concrete research report in your final answer:

- Current flow map with file:line anchors.
- Exact breakpoints causing "running session_id cannot be read/waited".
- Recommended ownership boundaries for wait module and exec handle.
- MVP implementation slices and risks.
- Tests required before implementation can be accepted.

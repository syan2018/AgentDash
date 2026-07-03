# Research Brief: Codex wait / exec closure reference

Active task: `.trellis/tasks/07-03-waitable-activity-exec-closure`

You are a read-only research worker. Do not edit files, do not commit, do not push.

## Objective

Inspect `references/codex` and extract the capability model relevant to AgentDashboard's waitable activity + exec closure design.

We are not integrating Codex runtime or copying Codex identity types. We only need a grounded reference for:

- exec command lifecycle: start, running handle/session id, read output, wait for output/completion, stdin/write, terminate, status/exit code.
- wait semantics: waiting on activity, timeout behavior, immediate return when activity already exists, output/result summary versus full result transport.
- subagent/parallel work lifecycle: spawn/send/result/close/wait if present.
- mailbox/thread/event return model: how results are stored, referenced, and surfaced after wait.
- Windows/shell context hints if Codex has a way to shape command syntax for the active shell.

## Questions To Answer

1. Which files implement Codex exec/shell tool behavior? Include file paths and line anchors.
2. Which files implement wait behavior? Include file paths and line anchors.
3. What is the exact shape of returned data for running commands and completed commands?
4. How does Codex avoid losing output when a command continues running?
5. How does Codex handle wait timeout and result references?
6. What pieces are reusable as design ideas, and what must not be imported into AgentDashboard because of our constraints?
7. What minimal AgentDashboard API/tool surface would match the useful closure without copying Codex domain/runtime?

## Constraints

- AgentDashboard must not depend on Codex runtime.
- AgentDashboard must not introduce Codex Thread/AgentPath/domain identity.
- AgentDashboard must not add external `/sessions/*` endpoints.
- The proposed design must support a common wait module used by exec, companion/subagent, human/user response, mailbox/wake activity, and future parallel sources.

## Output

Write a concise but concrete research report in your final answer:

- Findings with file:line anchors.
- Reference capability flow diagram in text.
- Gaps between Codex and AgentDashboard.
- Recommended design lessons for AgentDashboard.
- Explicit "do not copy" list.

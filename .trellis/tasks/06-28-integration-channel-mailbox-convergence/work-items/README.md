# Work Items

本目录拆分当前 task 内的执行工作项。它们不是 Trellis 子任务；状态、依赖和验证都在当前 task 内追踪。

## Index

| Item | File | Summary |
| --- | --- | --- |
| W0 | [W0-source-schema-baseline.md](./W0-source-schema-baseline.md) | Mailbox source identity / envelope attribution model |
| W0A | [W0A-agent-run-mailbox-directory-split.md](./W0A-agent-run-mailbox-directory-split.md) | AgentRun mailbox application service 拆成目录模块 |
| W1 | [W1-mailbox-intake-command-shape.md](./W1-mailbox-intake-command-shape.md) | Routine / Companion 复用 mailbox intake 的 command shape |
| W2 | [W2-routine-reuse-mailbox.md](./W2-routine-reuse-mailbox.md) | Routine Reuse / repeated PerEntity 入 mailbox |
| W3 | [W3-companion-sub-dispatch.md](./W3-companion-sub-dispatch.md) | Companion sub child initial task 入 child mailbox |
| W4 | [W4-companion-child-result.md](./W4-companion-child-result.md) | Child result materialize 到 parent mailbox |
| W5 | [W5-companion-parent-request-response.md](./W5-companion-parent-request-response.md) | Parent request / response 双向 mailbox delivery |
| W6 | [W6-companion-human-request-response.md](./W6-companion-human-request-response.md) | Human response 入 requesting AgentRun mailbox |
| W7 | [W7-platform-boundary.md](./W7-platform-boundary.md) | Platform broker boundary |
| W8 | [W8-workspace-projection-ux.md](./W8-workspace-projection-ux.md) | Workspace projection / UX / contract check |

## Status Legend

- `planned`: 已定义，未实现。
- `in_progress`: 当前正在实现。
- `blocked`: 依赖未满足或实现中发现阻塞。
- `done`: 实现、验证、检查均完成。

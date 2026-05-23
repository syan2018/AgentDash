# Workflow/VFS/Relay 模块边界拆分

## Goal

按语义边界拆分 Workflow、VFS、Relay protocol 与 Agent loop 的大文件/大模块，降低后续维护成本，同时保持协议、数据库和行为稳定。

## Requirements

- 先做文件级/目录级拆分，不急着拆 crate。
- 保持 `mod.rs` re-export 或 facade，降低调用方改动。
- 每一批拆分只处理一个 bounded context，避免跨 Workflow/VFS/Relay/Agent loop 同时大改。
- 不改变 relay JSON 协议形态、workflow persisted schema、VFS mount 语义。
- 拆分后补充或运行相关测试，确认纯移动没有行为变化。

## Candidate Boundaries

| 区域 | 建议拆分 |
| --- | --- |
| Workflow value objects | `contract.rs`、`lifecycle_def.rs`、`activity_def.rs`、`run_state.rs`、`tool_capability.rs`、`hook_rule.rs`、`mount_directive.rs`、`validation.rs` |
| VFS | `core`、`providers`、`tools`、`mutation`、`materialization`、`surface` |
| Relay protocol | `handshake`、`prompt`、`workspace`、`tool`、`mcp`、`terminal`、`session_event`、`capabilities` |
| Agent loop | `turn`、`tool_call`、`event_mapping`、`cancellation`、`prompt`、`output` |

## Acceptance Criteria

- [ ] 拆分顺序明确，避免在一个 PR 中同时移动所有大模块。
- [ ] 至少完成一个高收益模块的目录级拆分，或形成多个子任务。
- [ ] 拆分后 public import/re-export 关系清晰。
- [ ] 相关测试、typecheck 或 cargo check 通过。
- [ ] spec 或 review 文档记录新的模块边界原因。

## Out of Scope

- 不做 crate 级拆分，除非另开任务。
- 不借拆文件机会改变业务逻辑。
- 不修改 relay protocol wire format。

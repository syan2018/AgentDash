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

- [x] `workflow/value_objects.rs` 按 contract、lifecycle、activity、run state、capability、hook/mount directive 等语义边界拆分，`value_objects.rs` 只保留 facade/re-export 或极少量聚合入口。
- [x] `vfs/tools/fs.rs` 按 read/list/search/write/patch/shell 等 agent-callable tool handler 拆分，并复用统一路径解析与 tool result helper。
- [x] `agentdash-relay/src/protocol.rs` 保留顶层 `RelayMessage` 信封，prompt、workspace、tool、mcp、terminal、session event、capability 等 payload 迁入子模块；wire format 不变。
- [x] `agentdash-agent/src/agent_loop.rs` 按 turn、tool call、event mapping、cancellation、prompt/output 等内部边界拆分，主文件只保留 loop orchestration。
- [x] 拆分后 public import/re-export 关系清晰，调用方不需要感知内部物理文件迁移。
- [x] 每批拆分后运行相关 cargo test/check，最终运行覆盖四个 crate 的 check。
- [x] spec 或 review 文档记录新的模块边界原因。

## Out of Scope

- 不做 crate 级拆分，除非另开任务。
- 不借拆文件机会改变业务逻辑。
- 不修改 relay protocol wire format。

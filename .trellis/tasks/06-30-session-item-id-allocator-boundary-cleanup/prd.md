# Session item id allocator 边界清理

## Goal

把工具结果和终端输出使用的 session scoped item id 分配从 AgentLoop / Pi connector 的临时职责中收束出来，形成清晰的会话运行时身份分配边界。完成后，AgentLoop 只消费已分配的 tool result address，Pi connector 只注入恢复后的运行时状态，`ThreadItem.id`、tool result cache key 与 lifecycle VFS path 继续共享同一个 session item id。

## Background

刚修复的冷启动恢复 bug 说明了当前边界问题：重启后 PiAgent 新建 in-process runtime 时，进程内 `ReadableIdRegistry` 丢失，导致新工具结果复用历史 `turn_001:tool_004` 这类 id，前端按同一个 `ThreadItem.id` 更新旧 card。止血修复已经从 `RestoredSessionState.messages` 推进 registry 计数器，避免 id 碰撞；本任务负责把这套身份分配机制移到更合适的 session runtime / projection 边界。

已确认事实：

- `.trellis/spec/backend/session/pi-agent-streaming.md` 规定 tool result 的 readable item id 同时用于 `SessionToolResultCache` key、Backbone `ThreadItem.id` 与 lifecycle VFS 分段路径。
- `.trellis/spec/cross-layer/backbone-protocol.md` 规定 PiAgent 工具结果的 `ThreadItem.id` 与 `lifecycle_path` item id 必须同源，形状为 `{turn_alias}:{body_alias}`。
- 当前 `ReadableIdRegistry` 位于 `crates/agentdash-agent/src/agent_loop.rs`，但它管理的是 session projection 可见的 id，而不是 AgentLoop 的核心执行语义。
- 当前 Pi connector 能看到 `RestoredSessionState` 和 runtime 创建点，因此止血修复放在 connector 内；长期边界上，connector 不应负责解析历史 message details JSON 来恢复 allocator 状态。

## Requirements

- R1. 提供一个 session item identity / tool result address 分配边界，拥有 `{turn_alias}:{body_alias}`、tool/cmd 分类、terminal alias、lifecycle path 和 cache key 相关格式。
- R2. AgentLoop 不再直接拥有 readable item id 的计数器、格式化和历史观测逻辑；AgentLoop 通过注入的 allocator/address provider 获取 tool result ref。
- R3. PiAgent runtime 创建时接收已恢复的 allocator state 或 allocator 实例；Pi connector 不再内联解析 `readable_ref` / `lifecycle_path` JSON 来推进计数器。
- R4. repository restore / session rehydrate 路径负责从持久化 transcript 或 projection 事实中恢复 allocator watermark，并以 typed state 交给 runtime。
- R5. 现有跨层协议保持不变：同一次 tool result 的 `ThreadItem.id`、`details.readable_ref.item_id`、bounded text 中的 `lifecycle_path` 与 cache key item id 必须一致。
- R6. 冷启动恢复已有会话后，新工具调用和 shell command 必须继续分配 session 内唯一的新 id，不能覆盖历史工具 card。
- R7. 预研项目内可直接更新 Rust API、测试和 specs，不需要兼容旧 API 包装层。

## Acceptance Criteria

- [ ] `ReadableIdRegistry` 或其继任类型不再定义在 `agent_loop.rs`，AgentLoop 文件中不再包含 `{turn_alias}:{body_alias}` 的格式化/解析职责。
- [ ] `ToolResultRefContext` 或等价上下文通过 allocator/address provider 注入 tool result ref，AgentLoop 只使用返回的 `item_id` 与 `lifecycle_path`。
- [ ] Pi connector 的 prompt 冷启动路径不再直接扫描 `AgentMessage.details` JSON；恢复逻辑位于 session restore / identity 模块，并暴露 typed runtime state。
- [ ] repository-restored prompt 测试覆盖历史 `turn_001:tool_004`、`turn_002:cmd_002` 后继续生成 `turn_003:tool_005`、`turn_003:cmd_003`。
- [ ] stream mapper 测试继续证明 tool result `ThreadItem.id` 与 `lifecycle_path` 内嵌 item id 一致。
- [ ] lifecycle VFS / `SessionToolResultCache` 测试继续证明 cache key 与 readable item id 同源。
- [ ] 相关 backend session 与 cross-layer specs 更新为新的 ownership 描述。
- [ ] `cargo fmt`、聚焦 Rust 测试和 `git diff --check` 通过。

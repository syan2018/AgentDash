# Agent Runtime 架构基线调查

## 调查范围

本文件记录主会话对当前 crate 依赖、application composition、Agent Core、executor、RuntimeSession、relay 与 Codex App Server Protocol 的交叉调查。结论以当前代码为事实源，历史任务与 spec 只用于解释设计意图。

## 已确认的结构事实

### 1. 当前 application-facing seam 不是完整的 Agent Runtime interface

- `agentdash-spi::AgentConnector` 定义在 `crates/agentdash-spi/src/connector/mod.rs:979`，同时承载 executor discovery、live-session 查询、prompt、cancel、steer、工具审批、工具热更新与 out-of-band notification。
- `ConnectorCapabilities` 只在 `crates/agentdash-spi/src/connector/mod.rs:42` 提供 7 个布尔字段，不能表达 Codex App Server Protocol 已有的 `thread/resume`、`thread/read`、`thread/fork`、`thread/compact/start`、`turn/interrupt`、server request/response、context ownership 与 snapshot fidelity。
- `ExecutionTurnFrame` / `ExecutionContext` 分别位于 `crates/agentdash-spi/src/connector/mod.rs:237` 和 `:262`。它们把 hook runtime、runtime delegate、CapabilityState、restore state、ContextFrame、tool instances、VFS、MCP、backend lease 与业务身份一次性暴露给所有 connector；调用方必须理解的 interface 已接近 application runtime 的实现复杂度。

### 2. 内部 Agent 与外部 Agent 只统一了事件外形，没有统一命令和状态语义

- `PiAgentConnector` 在 `crates/agentdash-executor/src/connectors/pi_agent/connector.rs:472` 实现 `AgentConnector`，内部持有 `agentdash-agent::Agent`，把 core `AgentEvent` 映射为 Backbone。
- `CodexBridgeConnector` 在 `crates/agentdash-executor/src/connectors/codex_bridge.rs:569` 实现同一 trait，但直接管理 app-server JSON-RPC、thread/turn id 与通知映射。
- 两者都返回 `ExecutionStream<BackboneEnvelope>`，但 Pi 的 thread history、context projection、compaction 和 runtime delegate 由 AgentDash 管理；Codex 的 thread state 与 compaction 由外部 app-server 管理。当前统一只发生在输出事件，不发生在操作集合、恢复语义、上下文所有权或失败边界。
- Relay 第三个 adapter 甚至位于 application crate：`crates/agentdash-application/src/relay_connector.rs:27`。这使 infrastructure adapter ownership 穿过 application，且 cloud/local 两侧都会启动一套 `application-runtime-session`。

### 3. Codex adapter 未消费已依赖协议的完整能力

- workspace 编译依赖固定在 `codex-app-server-protocol` `rust-v0.140.0`（根 `Cargo.toml:123`），Codex adapter 实际启动 `@openai/codex@0.124.0`（`crates/agentdash-executor/src/connectors/codex_bridge.rs:641`），编译期类型和运行时 server 存在版本错位。
- `build_prompt_text`（`codex_bridge.rs:176`）把结构化 input 与 ContextFrame 合成单个文本；图片、typed context channel 和业务来源信息无法按原协议能力传递。
- `build_thread_start_params`（`codex_bridge.rs:242`）只映射少量 model/cwd/approval/sandbox 配置，未使用协议已提供的 developer instructions、runtime workspace roots、dynamic tools 等能力。
- `handle_server_request`（`codex_bridge.rs:555`）仍自动接受 command/file approval，并对 user-input request 返回空答案；`AgentConnector::approve_tool_call/reject_tool_call` 反而未接到这些 pending JSON-RPC request。
- 当前 adapter 没有 application-facing `thread/resume/read/compact` 或 `turn/interrupt` 操作。`thread/compacted` 只被转成遥测事件，无法由 AgentRun 通过通用 command interface 发起。
- 每次 `prompt()` 都启动新 app-server 进程，`follow_up_session_id` 被用于 fork，而不是对已绑定 thread 做标准 resume；外部 thread 的长生命周期没有成为 executor module 隐藏的实现细节。

### 4. Agent Core 仍包含 AgentDash runtime 业务

- `agentdash-agent` 的 `LlmBridge` interface 位于 `crates/agentdash-agent/src/bridge.rs:306`，这一 provider seam 本身合理。
- 但 `agentdash-agent/src/compaction/mod.rs` 同时拥有 compaction eligibility、摘要 provider 调用、replacement history、AgentDash Lifecycle 回看索引与项目专用摘要 prompt。这些属于 managed Agent runtime 的会话业务，不是纯 provider/tool loop 必须知道的内容。
- `RuntimeCompactionDelegate` 与 `AgentRuntimeDelegateSet` 分别位于 `crates/agentdash-agent-types/src/runtime/delegate.rs:22`、`:116`。core interface 因而直接知道 manual/auto compaction、context transform、AgentRun admission、mailbox turn boundary 与 provider observer 等 application runtime concern。
- `agentdash-agent-types/Cargo.toml` 直接依赖 `codex-app-server-protocol`，`UserInputBlock` 又在 `crates/agentdash-agent-protocol/src/backbone/user_input.rs:16` 直接 alias `codex::UserInput`。所谓 core/model 层并未与第三方 wire protocol 隔离。

### 5. `application-runtime-session` 同时承担 runtime engine、application orchestration 与 projection infrastructure

- `SessionRuntimeBuilder` 位于 `crates/agentdash-application-runtime-session/src/session/runtime_builder.rs:36`，通过大量 `with_*` 与异步 `set_*` 注入 connector、stores、VFS、MCP、AgentFrame、anchor、AgentRun capability、hook target、mailbox、terminal control effects、manual compaction request repo 等依赖。
- API bootstrap 先从 builder 取出 session services，再用这些 services 构建 AgentRun mailbox/control adapter，最后把 adapter 写回 builder。crate 依赖图虽无循环，运行时对象装配存在显式回环和顺序约束。
- `ContextProjector`（`crates/agentdash-application-runtime-session/src/session/context_projector.rs:20`）负责 durable model context 重建；职责本身稳定，但它位于 application session crate，且和 connector launch、AgentRun command、HTTP read model 共处同一大 module。
- `SessionEventingService`（`crates/agentdash-application-runtime-session/src/session/eventing.rs:59`）同时承担 durable/ephemeral 分类、事件写入和广播、title projection、projection head 推进、rewind、oversized payload guard，以及 compaction 原子提交。
- `maybe_commit_compaction_projection`（`eventing.rs:622`）还直接终结 manual compaction domain request。runtime event ingestion 因而知道 AgentRun command lifecycle repository。

### 6. 手动 compaction 是 application 与 native runtime 私有耦合，而不是通用 runtime operation

- AgentRun 定义了窄的 `AgentRunContextCompactionRuntimePort`（`crates/agentdash-application-agentrun/src/agent_run/context_compaction_command.rs:176`），但唯一真实 adapter `AgentRunContextCompactionSessionRuntimePort`（`:184`）直接依赖 `SessionLaunchService` 和 manual request repository。
- compact-only 通过特殊 `LaunchSource::ContextCompaction` 和一段 maintenance prompt 启动；native agent loop 通过 `ManualContextCompactionDelegate` 消费 repository request。
- 该链路无法调用 Codex 的 `thread/compact/start`，也不能让其他 executor 通过相同 interface 声明或实现 compact。
- 成功的 projection commit 必须继续是 native AgentDash context 的正确边界；问题在于它应被 managed runtime module 隐藏，并通过统一 operation/event correlation 反馈 application，而不是让 runtime delegate 读取 AgentRun repository。

### 7. Relay 传输复制了 session runtime 与协议转换

- Cloud 侧 `RelayAgentConnector` 把 `ExecutionContext` 手工映射为 `RelayPromptRequest`；local 侧 `PromptCommandHandler` 再组装 `LaunchCommand` 并启动本机 `SessionRuntimeServices`。
- local 将 typed `BackboneEnvelope` 序列化为 `serde_json::Value`，cloud WebSocket handler 再反序列化为同一类型。relay transport 没有直接承载 canonical agent runtime command/event contract。
- cancel/steer/discovery 是平行 relay message，compact/resume/read/approval/user-input response 等操作没有对称传输面。

## Deep-module 删除测试

| 当前 module | 删除后的结果 | 判断 |
| --- | --- | --- |
| `AgentConnector` | discovery、thread/turn 控制、恢复、审批、热更新与事件流会重新散回 application 调用方 | seam 真实，但当前 interface 太宽且能力模型太浅 |
| `CompositeConnector` | executor id 路由会散回各入口 | 有价值，但应隐藏 capability negotiation、binding 与 adapter lifecycle，而不只是转发方法 |
| `application::runtime_session_agent_run_bridge` | 大量一一映射类型消失，调用方可直接共享 application-owned runtime port | pass-through，目标态应删除 |
| `SessionControlService` | 方法只是转调 connector | shallow module；控制应进入深的 Agent Runtime interface |
| `ContextProjector` | checkpoint + suffix 的恢复算法会散回 launch、query、fork 与 compact | deep module，应该保留行为但移动到正确 owner |
| `SessionEventingService` | 多种无关复杂性会散到不同 owner，而不是同一问题的重复实现 | 当前是职责聚合，不是有意隐藏的单一 deep module，需要按事实提交、live fanout、projection 分解为内部 module |
| `AgentRunContextCompactionSessionRuntimePort` | 特殊 launch prompt、短轮询与 repository request 耦合直接消失 | pass-through/补丁 seam，应由通用 compact operation 取代 |

## 依赖类别

- In-process：Agent core loop、native runtime state machine、context composer、capability reducer。
- Local-substitutable：runtime event/projection persistence；已有内存 fixture 与 PostgreSQL adapter，可通过 runtime interface 做一致性测试。
- Remote but owned：Cloud ↔ Local relay；应传输 canonical runtime commands/events，由 WebSocket adapter 实现同一 port。
- True external：Codex app-server subprocess 与 LLM provider；executor/native runtime 分别通过 adapter/mock 隔离。

## 仍需产品意图决定的问题

1. “内部与外部 Agent 能力对齐”是指统一操作与可观察生命周期，还是要求 AgentDash 对外部 Agent 也拥有逐消息、可精确 replay 的模型上下文事实源？Codex app-server 可以 resume/read/compact，但其公开 thread history 与 provider 内部 replacement context 并不等同于 AgentDash native projection。
2. AgentRun mailbox/hook 在 native core 内部的同步 turn-boundary 行为，是否允许调整为统一协议可表达的 turn/steer/interaction 行为？若必须保留 core-only 回调，则外部 executor 永远无法获得相同语义。

## 初步架构约束（非方案）

- application 只能依赖项目拥有的 Agent Runtime interface，不能依赖 Codex/ACP wire types、connector 实现或 native core delegate。
- 至少已有 native runtime、Codex app-server、remote relay 三个真实 adapter，因此 Agent Runtime seam 不是假设抽象。
- Agent Core 不依赖 application/domain/persistence/Backbone/Codex；compaction、thread/turn/item、context projection 与 runtime interaction 属于 core 之上的 managed runtime。
- executor 隐藏 adapter 选择、外部进程、remote transport、capability negotiation 与 executor thread binding。
- unsupported capability 必须在副作用前显式拒绝；不引入文本降级、noop fallback 或假能力对齐。
- protocol command、response、notification、server request 与 response 必须成对设计，不能继续只有事件 backbone、命令散落 trait method。

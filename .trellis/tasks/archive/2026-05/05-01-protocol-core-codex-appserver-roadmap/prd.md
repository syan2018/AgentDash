# 自维护协议基底规划（Codex App Server Protocol）

## Goal

以 Codex App Server Protocol 的 thread/turn/item 语义为运行时事实主干，全链路替换当前 ACP SessionNotification 体系；ACP 仅保留为 API 层对外嵌入 facade。

## 现状与核心问题

### 已完成

* `CodexBridgeConnector` 已直接引入 `codex-app-server-protocol` (rust-v0.121.0) crate
* 实现了 initialize → thread/turn 创建 → 事件流转 → cancel 的完整 JSON-RPC 生命周期
* 映射了 agent_message/reasoning/token_usage/turn_completed/error 五种核心通知到 ACP

### 待解决

| # | 问题 | 影响 |
|---|------|------|
| 1 | 每次 `prompt()` spawn 新 `npx codex app-server` 进程 | 冷启动延迟高、npm 下载开销（推迟到链路替换后决定：A 长驻进程或 C codex-rs in-process） |
| 2 | 只映射 5/30+ 种事件，工具调用/文件变更/plan/item 生命周期被丢弃 | 前端看不到中间操作过程 |
| 3 | 审批链路硬编码 auto-accept | 执行器不可控 |
| 4 | `follow_up_session_id` 被直接当作 Codex `thread_id` | 标识域混淆 |
| 5 | 全链路（session hub / persistence / 前端）仍深度依赖 ACP `SessionNotification` | 替换工作量集中在 application 层 |

## 策略决策

**协议采用策略：默认直用 Codex 类型，仅在平台治理需要时添加最小 envelope**

* thread/turn/item 语义直接使用 `codex-app-server-protocol` 的 Rust 类型
* 不引入额外的"Runtime Backbone"抽象层——Codex 协议本身就是 backbone
* 平台仅在 trace / 来源标记 / 版本追踪等治理位添加 envelope 字段
* 版本跟踪：手动跟踪 Codex 发布，不需要自动化 pipeline（预研阶段）
* ACP 仅在 HTTP API 层做协议转换 facade，不进入内部处理链路

## Codex 协议事件全景（rust-v0.121.0）

### ServerNotification（server → client，共 ~30 种）

**当前已映射：**
- `item/agentMessage/delta` → ACP AgentMessageChunk
- `item/reasoning/textDelta` → ACP AgentThoughtChunk
- `item/reasoning/summaryTextDelta` → ACP AgentThoughtChunk
- `thread/tokenUsage/updated` → ACP UsageUpdate
- `turn/completed` (Failed) → ConnectorError
- `error` → ConnectorError

**需要映射（P0 — 直接影响用户体验）：**
- `item/started` — item 生命周期开始（tool call / file change / command 等）
- `item/completed` — item 生命周期结束
- `item/commandExecution/outputDelta` — 命令执行输出流
- `item/fileChange/outputDelta` — 文件变更输出流
- `turn/started` — turn 生命周期
- `turn/diff/updated` — turn 级文件 diff 汇总

**可延后映射（P1 — 增强体验）：**
- `item/mcpToolCall/progress` — MCP 工具调用进度
- `turn/plan/updated` — plan 变更
- `item/plan/delta` — plan 流式 delta
- `item/commandExecution/terminalInteraction` — 终端交互
- `thread/status/changed` — 线程状态变化
- `thread/compacted` — 上下文压缩
- `model/rerouted` — 模型路由变更

**不需要映射（平台不使用）：**
- `thread/started/archived/unarchived/closed` — 由平台自己管理 thread 生命周期
- `thread/name/updated` — 平台有自己的标题生成
- `account/*` — 账户管理走平台层
- `thread/realtime/*` — 实验性语音功能
- `windowsSandbox/*` — 平台环境设置
- `fuzzyFileSearch/*` — 文件搜索走平台层
- `skills/changed` — 技能管理走平台层

### ServerRequest（server → client，共 7 种）

**需要正式化：**
- `item/commandExecution/requestApproval` — 当前硬编码 auto-accept
- `item/fileChange/requestApproval` — 当前硬编码 auto-accept
- `item/tool/requestUserInput` — 当前返回空
- `item/permissions/requestApproval` — 未处理

**可延后：**
- `item/tool/call` — 动态工具调用（需要前端 UI 支持）
- `mcpServer/elicitation/request` — MCP 服务器交互
- `account/chatgptAuthTokens/refresh` — 认证刷新

## 任务拆分

### P0: backbone-event-model（事件模型 + Codex 映射）

定义平台内部事件类型体系，完成 Codex 事件到内部类型的映射规则。

### P1: codex-bridge-completion（bridge 实现补齐，推迟到链路替换后）

- 补齐 P0 事件的实际映射代码
- 正式化审批请求链路（对接前端审批 UI）
- 修复 session_id ↔ thread_id 标识域
- 进程模型优化（长驻进程或 in-process，待评估编译成本后决定）

### P2: rust-ts-protocol-binding（类型绑定直出）

backbone 事件类型通过 rs-ts 直出到前端，前端直接消费原始事件类型。

### P3: acp-api-facade（API 层协议转换）

ACP 仅作为 HTTP API 层 facade，在 `acp_sessions` 路由做协议转换。

## 里程碑

* **M1**: P0 完成 — 事件模型定义 + 映射规则文档
* **M2**: P1 完成 — codex_bridge 可完整运行，前端可看到工具调用/文件变更过程
* **M3**: P2 完成 — 前端消费 backbone 原始类型
* **M4**: P3 完成 — ACP 退出内部链路，仅保留 API facade

## Out of Scope

* 自动化 schema 跟踪 pipeline（手动跟踪即可）
* Conformance 自动化测试框架（等协议稳定后再补）
* 多 profile 并行治理
* 跨所有执行器一次性适配

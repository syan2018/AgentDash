# 统一 Agent Runtime View 状态链路并修复停止按钮

## Goal

建立一条由 concrete Complete Agent authority 驱动的 AgentRun 读取链路，使消息呈现与执行控制从同一次 Agent read/change observation 收敛，同时保持 Product Workspace 的职责清晰。修复运行中停止按钮仍显示为发送箭头、无法取消回合的问题。

用户价值：

- 会话执行状态和可用命令始终与 Complete Agent authority 的 Runtime view 一致。
- Conversation Feed 的压缩、重载、去重和临时展示变化不会影响执行控制。
- 前端不再从展示事件推断 Workspace 控制状态。

## Background

- `ComposerSendButton` 只在 Workspace execution 为运行态且输入为空时显示停止按钮。
- 当前 Workspace Control Plane 通过 Conversation Feed 的 `turn_started` 等展示事件触发 Workspace snapshot 刷新。
- `useSessionStream` 当前按 `conversation_history` 数组位置生成临时 `event_seq`；权威快照收敛后数组可能缩短，而 `SessionChatView` 的 live side-effect cursor 保持旧值，导致下一次 `turn_started` 被过滤。
- 已对实际问题会话的持久化记录做只读检查：后端提交了 `turn_started`、active turn 变更和后续终态，问题发生在前端事实同步链路。
- 最小诊断测试已复现“旧游标大于收敛后新事件序号，下一次 `turn_started` 不被派发”。
- Runtime 已存在 Managed Runtime snapshot、live event sequence、Workspace snapshot/control plane 等模型，但当前前端在多个 Hook 之间分别编排。
- 当前名为 `ManagedRuntimeSnapshot` 的 DTO 同时包含 `revision`、lifecycle、interactions、command availability 与 canonical conversation history；它是 Agent Runtime 从 Complete Agent authority 即时 normalize 的可重建 read model，不是 Runtime-owned aggregate。
- Complete Agent 的 `AgentChangePayload::SourceObservation` 已声明“一次 source observation 原子更新 normalized state，并附带零个或多个 presentation records”，这是状态与呈现共同收敛的正确变更单位。
- 当前 `/runtime/live` 暴露的 `AgentLiveEvent` 按契约只是 process-local presentation data，`sequence` 可在进程重启后重置，不能充当 Runtime 控制事实或 durable revision。
- Product Workspace 查询目前从 `ManagedRuntimeSnapshot` 还原 `AgentObservation`，再重新派生 execution、conversation commands 和 Workspace state；前端同时维护 Managed Runtime feed 与 Workspace HTTP state，形成重复投影和跨 Hook 时序协调。
- 当前领域词汇已明确：AgentRun Product 是产品 aggregate；concrete Complete Agent 是 history、execution 与 effect 的唯一事实 owner；Agent Runtime 只是进程内解析、协调、协议映射、normalize 与 broadcast 机制，不拥有跨重启状态。
- Agent Runtime 的公开 seam 面向 Application 层模块（AgentRun、Lifecycle、Workflow、Companion、VFS/Tool/Hook）；Complete Agent service/source 位于 Runtime Host 与 Integration adapter 后面，不进入 Product UI 语言。
- `.trellis/spec/project-overview.md` 仍将 Managed Agent Runtime 描述为唯一执行事实源，与根 `CONTEXT.md`、近期架构收口任务及当前代码职责冲突，需要在本任务中修正规范。

## Requirements

- R1：execution、active turn、command availability 必须来自唯一的 concrete Complete Agent authoritative read/change，不得从 Conversation Feed 展示记录推断。
- R2：前端应提供一个小而稳定的 Agent Runtime view 接口，统一承载当前 normalized Agent read model、source revision/change cursor、连接状态和刷新/命令入口；调用方无需自行协调 Feed 与 Workspace 的时序。该接口是可重建 read module，不形成新的领域 aggregate。
- R3：Conversation Feed 继续负责 canonical conversation presentation；它可与 Workspace 视图独立投影，但必须消费同一 Runtime target 和版本坐标。
- R4：Conversation Feed 的压缩、终态收敛、重连和展示记录替换不得改变执行事实或丢失控制状态更新。
- R5：Composer 只消费权威 execution 和 command availability；运行中且可取消时展示停止按钮并可成功发起取消。
- R6：删除以数组下标充当跨快照 live event identity/cursor 的控制用途。
- R7：不引入 `isReceiving`、定时轮询或 Conversation Feed 生命周期推断等兼容/回退线路。
- R8：保持现有 Runtime target/binding authority，不复制 source identity 或另建平行状态 owner。
- R9：Agent Runtime 内部以 concrete Complete Agent `read/changes` 作为权威恢复链；统一 `AgentRuntimeConnection` 负责 normalized baseline、Agent change tail、gap reload、process-local presentation overlay 和命令可用性收敛。
- R10：统一连接模块通过 `AgentRuntimeView` 暴露面向调用方的 selectors；Conversation Feed、Composer 和 Agent 状态 UI 不得分别建立连接、游标或刷新编排。
- R11：Product Workspace 只拥有 Product shell、AgentFrame、resource surface、subject/lineage 和 Workspace Module 等 Product/Resource 投影；Runtime execution、active turn 和 Runtime command availability 不得在 Workspace 中重复成为状态 owner。
- R12：不引入公开的 `RuntimeSession` 或 Complete Agent 领域术语。清理会暗示 Runtime 拥有 durable aggregate/state 的 `ManagedRuntimeSnapshot` 命名；应用/前端 read model 使用 `AgentRuntimeView`，连接实现使用 `AgentRuntimeConnection`，`AgentRunTarget` 只作为解析 Product association 的查询坐标。

## Acceptance Criteria

- [ ] AC1：收到权威运行态 observation 后，Composer 显示停止按钮，cancel command 可用。
- [ ] AC2：执行取消后，Composer 和 Runtime control view 按新的权威 observation 收敛到非运行态。
- [ ] AC3：终态权威快照收缩 Conversation history 后，下一轮开始仍能立即获得正确运行态和停止按钮。
- [ ] AC4：Conversation Feed 压缩、reload、reconnect 不会驱动或回退 Runtime execution 状态。
- [ ] AC5：代码中不存在依赖 `conversation_history` 数组下标推进控制面副作用游标的路径。
- [ ] AC6：覆盖统一状态接口、跨 revision 收敛、下一轮开始和取消命令的前端测试；涉及后端契约变化时补充对应 Rust 合约/集成测试。
- [ ] AC7：相关前端 type-check、lint 和定向测试通过。
- [ ] AC8：同一 AgentRun target 只建立一条 `AgentRuntimeConnection`；Feed 与 Composer 从同一次 Agent read/change observation 读取。
- [ ] AC9：Product Workspace 的 reload、失败或刷新状态不改变 Composer 的 execution/cancel 状态。
- [ ] AC10：代码和规范中不存在将 Runtime Session/Managed Runtime snapshot 描述为新的执行事实 owner 的定义。

## Out of Scope

- 改变 Agent 执行、取消或 Conversation canonical journal 的业务语义。
- 为旧状态链路保留兼容或回退实现。
- 与本问题无关的 Workspace Module、Surface 或 Session UI 重构。

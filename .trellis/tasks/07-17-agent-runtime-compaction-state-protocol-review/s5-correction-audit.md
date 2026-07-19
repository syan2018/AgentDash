# S5 纠偏审计

> 状态：执行中  
> 基准：S5 合入第一父提交 `21a30907`、07-12 canonical conversation contract、07-17
> `prd.md` / `design.md` / `implement.md`  
> 目的：区分最终架构迁移、尚未完成替代的产品能力和无替代误删，恢复 S5 的真实
> activation 边界。

## 1. S5 的原始职责

S5 是一次原子 activation：

```text
已独立验证的 owner component
  -> production caller 切换
  -> composition / repository / schema 接通
  -> 行为 tracer 证明新路径完整
  -> 旧 RuntimeSession / universal journal / duplicate owner 零消费者
  -> 物理删除旧实现
```

Hard Cut owner 只拥有 migration、workspace/Cargo、composition、跨 owner 集成和已经
零消费者的删除热点。它不拥有 Product 领域语义，也不能通过取消模块声明、取消 API
route 或删除 caller 来制造“零消费者”。

S5 必须保持的产品能力至少包括：

- Project Agent / AgentRun 创建、继续、删除、Fork；
- Companion dispatch、child AgentRun、channel、gate/adoption、result/mailbox；
- Workflow AgentCall、Routine、Story/Task execution；
- Capability/Tool/Hook/VFS/Workspace/Canvas surface；
- Wait activity、Terminal、Lifecycle view/runtime trace；
- 07-12 canonical App Server conversation presentation 与前端 reducer/card ecosystem。

这些能力可以更换底层 Runtime/Agent seam，不能从产品面消失。

## 2. 已确认的执行偏差

### 2.1 `8d22d6cf`

该提交同时做了两种性质不同的修改：

- 将旧 Hook/RuntimeSession 责任迁向 Complete Agent Host，这是目标方向；
- 从 `agentdash-application/lib.rs` 取消
  `companion`、`capability`、`runtime_tools`、`gate_wait_policy`、`wait_activity`、
  `routine`、`frame_construction` 等产品模块。

第二部分没有经过替代链路验收，属于错误 activation。模块被取消声明后成为“未编译
源码”，不能据此判定为死代码。

### 2.2 `a535ae01`

该提交建立 Complete Agent / Managed Runtime / Product projection 新组合的部分应保留，
但同时：

- 从 Router 移除 Companion、Routine、Canvas、Terminal、VFS surface、Workspace
  module、AgentRun workspace/runtime trace 等产品入口；
- 从 AppState 移除 Companion preflight、collaboration tool、parent mailbox、
  gate wake、wait/terminal convergence 等产品组合；
- 以“只保留最终投影查询”替代完整产品 API。

这不是 caller cutover，而是 caller deletion。没有对应新入口与行为 tracer 的部分必须
恢复。

### 2.3 `dfb4f903`

Extension Gateway rename 与旧 RuntimeSession/connector owner 清理方向正确；但一次删除
了约 4.5 万行，并把 AgentRun workspace、mailbox、surface、VFS access 等产品行为与旧
Runtime implementation 一并删除。每个删除项必须按业务能力逐项证明已有 final owner，
不能把整个提交当作已验收 hard cut。

### 2.4 `e1688dc7`

旧 Lifecycle VFS provider 依赖 journal，不能作为最终实现；但在 final
`LifecycleHistoryProjection -> LifecycleMountProvider` 尚未注册、mount 尚未进入
AgentRun resource surface 前物理删除，使 Lifecycle VFS 行为断链。该能力需要以
canonical Runtime history 重建并通过真实 VFS tracer，而不是保持删除状态。

### 2.5 `088fa55b`

该提交以“未进入模块树”为理由物理删除 Companion、frame construction、Routine 等约
1.7 万行。模块树缺席由 `8d22d6cf` 人为造成，且没有完整替代：

- Companion target saga 只有内存 repository/test component；
- `companion_fresh_saga` 有表但无 PostgreSQL repository；
- 无 production coordinator/worker/command/tool 接线；
- channel registry 仍在，但 Companion dispatch/result/gate/adoption 没有新闭环；
- Routine 没有 final replacement；
- Dash Agent 没有平台 collaboration tool 的生产入口。

因此 Companion/Routine 删除属于明确误删。旧源码只作为语义清单恢复；RuntimeSession、
journal、Backbone 依赖应在接入 final owner 时直接改写。

### 2.6 Protocol / frontend 删除

`d7144417`、`980b5f50`、`8d81020a` 基于“删除全局 protocol、由 Managed Runtime item
直接替代前端会话生态”的假设。该假设违反 07-12：

- `agentdash-agent-protocol` 是 AgentDash-owned canonical App Server protocol owner；
- Managed Runtime 只承载 source-authoritative snapshot/change 和 exact canonical
  conversation records；
- 前端继续消费 canonical conversation reducer/card ecosystem；
- Product feeds 与 conversation records 分离。

`8d81020a` 已由 `7d399de4` 回退；其余由 canonical protocol rewire 完成纠偏。

## 3. 当前能力状态

| 能力 | 当前状态 | 纠偏动作 |
| --- | --- | --- |
| Complete Agent / Managed Runtime 核心 | 已有最终骨架 | 保留并补 conformance |
| canonical App Server conversation | 正在恢复 owner 与 source projection | 完成 Rust/TS 单一 codegen 与前端接线 |
| Fork | target saga/Runtime seam 已有 | 补 production command、PG、真实 child tracer |
| Companion | target component 存在，产品链断开 | 恢复 Product/channel/tool/gate/result 全闭环 |
| Dash Agent subagent | 只有观察类型，无平台 spawn tool | collaboration tool 接 Product Companion command |
| Workflow AgentCall | durable component 已有 | 核验 production route、binding、recovery |
| Routine | 产品实现被误删 | 恢复入口并迁到 final AgentRun command |
| Lifecycle VFS | provider/query 已重建，mount activation 待接 | 接 AppliedResourceSurface 并做真实 read/list tracer |
| Terminal / Workspace / Canvas / VFS surface | 文件部分保留，路由/组合不完整 | 恢复路由并切 final owner |
| Wait / Capability / Runtime tools | 源码部分保留但未进入模块树 | 恢复模块与 final Tool Broker contribution |
| universal journal | 应删除 | 所有读取改为 Runtime snapshot/change + canonical history |

## 4. 回退与恢复原则

1. 不回退已验证的 Complete Agent、Managed Runtime、canonical u64、Runtime Wire、
   Product PostgreSQL 新合同。
2. 对混合提交不做盲目整提交 revert；恢复其中被删除的 Product capability，并直接接到
   final owner。
3. 每恢复一个产品面，同时恢复其 route、composition、permission、persistence、
   frontend consumer 与 tracer；不再制造“源码存在但未挂载”的假恢复。
4. 旧实现只作为业务语义和测试 oracle；不得恢复 RuntimeSession、universal journal、
   duplicate Backbone owner 或第二条 production path。
5. 每个阶段先证明新链功能完整，再删除对应旧实现；删除证据必须是 caller 已迁移，而
   不是 module/route 被手工摘除。

## 5. 恢复执行顺序

1. canonical conversation owner、source projection、Runtime exact history、前端 reducer；
2. Product 路由与 composition inventory 恢复；
3. Companion/channel/Dash collaboration tool 完整闭环；
4. Lifecycle VFS/AppliedResourceSurface 与 Runtime trace 读取闭环；
5. Routine、Wait、Capability、Workspace/Canvas/Terminal/VFS surface 逐项恢复；
6. Workflow AgentCall/Fork/Companion/Compaction/recovery 真实 tracer；
7. 只对已零消费者的 RuntimeSession/journal/connector/duplicate crate 做最终物理删除。

任何一步发现能力只有测试 component 而无 production caller，都不得标记为完成或继续
删除其旧产品入口。

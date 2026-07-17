# Frontend State Management

## Store Ownership

- Project/Story/Task/Lifecycle stores保存产品read model。
- AgentRun workspace state按`run_id + agent_id`保存当前产品shell、AgentFrame resource surface与Runtime inspect结果。
- Runtime feed hook保存snapshot baseline、durable/transient cursor与连接状态；它不复制后端state machine。Snapshot entries标记为durable，live transient按generation lane聚合。
- Workspace tab/layout store按AgentRun product key持久化用户布局，concrete presentation URI作为tab identity。
- 命令式 Tab 展示必须携带目标 workspace key，并通过
  `openOrActivateInWorkspace(workspaceKey, typeId, uri, options)` 在一次 store 操作中先绑定
  workspace 再打开 Tab。WorkspacePanel 的被动初始化 effect 必须读取 store 最新状态，
  原因是 history hydration 可能在 sibling effect 挂载前已经提交 presentation；使用首帧
  捕获的旧 workspace key 会把刚打开的 Tab 重置。

## Runtime Rules

- command enabled只来自canonical Runtime snapshot availability。
- target变化立即隔离旧snapshot/feed/resource surface；loading期间不泄漏前一target状态。
- reconnect携带durable cursor与transient generation/sequence，duplicate event不重复reduce；generation变化删除旧lane纯transient贡献但保留durable final，Lost和retention gap显示typed diagnostic。
- failed/cancelled/lost item按item identity终结原entry/card；final durable item覆盖过程delta，terminal后的stale delta不再修改展示。
- Backbone product/resource event只触发相应projection invalidate，不推进Runtime state。
- mailbox只显示queued intent与accepted Runtime operation；没有canonical endpoint的管理动作不进入model/intents。

必须测试target切换、stale snapshot、cursor replay、availability、presentation URI与layout稳定性；
命令式 presentation 还必须覆盖“先打开 Tab、后执行 WorkspacePanel 首次初始化”的顺序。

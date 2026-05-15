# Feasibility

## 可实现性结论

当前目标可实现，且与现有代码的迁移基础匹配：

- `UserPromptInput` 已经和 `PromptSessionRequest` 分离。
- `SessionAssemblyBuilder` / `PreparedSessionInputs` 已经沉淀了 surface 构建素材。
- `SessionBootstrapPlan` / `SessionContextSnapshot` 已证明 query 与启动共享派生数据是可行的。
- `ExecutionContext` 已分层，可由 launch 阶段的 connector input 投影生成，不需要先固定独立 `ExecutionPlan` 层。
- `TurnState` / `TurnExecution` 已经具备 runtime registry 的雏形。
- `session_events` 已经是可承接 runtime command event 与 terminal event 的事实基础。

## 迁移主线

1. 先锁定当前行为矩阵，尤其是入口、fallback、owner priority、connector failure、terminal effects。
2. 定义 `SessionConstructionPlan` 字段边界，并让 owner / VFS / MCP / capability / executor / context 的解析进入 construction trace。
3. 让 context endpoint、audit、inspector 投影 `SessionConstructionPlan`，收掉 query 与 launch 双路径。
4. 定义 `LaunchExecution`，把 lifecycle / restore / hook / follow-up / runtime command / terminal effect / connector input 从 pipeline 中移出。
5. 在 connector 边界由 `LaunchExecution` 投影 `ExecutionContext`，不把 projection 固化成主链路中间层。
6. 所有入口迁移为 `LaunchCommand` adapter，并删除生产主链路中的 `PromptSessionRequest`。
7. 拆 runtime registry / turn supervisor，保留 Turn 的薄边界。
8. terminal event 进入 outbox，移除 processor 内业务副作用。
9. runtime command 进入 domain event + derived projection，删除 meta hidden queue。
10. 删除有职责的 `SessionHub`，并收口 AppState ready builder、persistence store、working_dir 类型化。

## 实现约束

- 不新增跨域大对象。
- 不让 route/service/orchestrator 继续组装 session surface。
- 不让 `ExecutionContext` 反向成为 application 模型。
- 不让 projection 成为事实源。
- 不保留内存 callback 作为 terminal effect 唯一路径。
- 不先做只转发旧 request 的 launch service；每个切片必须减少一个旧分叉或旧可变壳。

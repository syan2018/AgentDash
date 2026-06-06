# 后续模块路线图

本文是 Dynamic Workflow / Lifecycle 后续模块的规划汇总入口。它承接已完成的 session-scoped API 机械迁移，并把后续工作拆成可独立验证的模块研究线。

## 当前状态

- 已完成 `agent-run-api-naming` 的第一步机械迁移：外露 command route 收敛到 `/sessions/{runtime_session_id}/...`。
- 当前任务已进入 `in_progress`，但后续模块仍以规划 / research 为主，不直接实现代码。
- `.trellis/config.yaml` 存在任务外本地变更，后续提交继续排除。
- 目标架构仍以 `Lifecycle` 作为完整上下文容器；`Orchestration` 是 `LifecycleRun` 内部的 0..N 状态容器，用于承载编译后的 plan、runtime node tree、dispatch、journal cursor 和 state exchange snapshot。

## 后续模块

| 模块 | 目标 | 预研文档 |
| --- | --- | --- |
| Orchestration domain contract | 落 `LifecycleContext`、`OrchestrationInstance`、`OrchestrationPlanSnapshot`、`RuntimeNodeState`、`StateExchangeSnapshot` 的最小合同与迁移字段 | `research/orchestration-domain-contract-plan.md` |
| WorkflowGraph compiler | 将现有静态 `WorkflowGraph` 编译为 `OrchestrationPlanSnapshot`，覆盖 Activity executor、transition、artifact、join/iteration policy | `research/workflow-graph-compiler-plan.md` |
| Common orchestration runtime | 用 common runtime snapshot/journal 执行静态 graph，并规划旧过程仓储的 projection / lease / trace index 收敛 | `research/common-runtime-convergence-plan.md` |

## 依赖关系

```text
agent-run-api-naming
  -> orchestration-domain-contract
  -> workflow-graph-compiler
  -> common-orchestration-runtime-static-graph
  -> dynamic-script-artifact-compiler
```

关键依赖不是 task tree 位置，而是 contract 成熟度：

- `workflow-graph-compiler` 依赖 `OrchestrationPlanSnapshot`、`PlanNode`、`ActivationRule`、`ExecutorSpec` 的最小合同。
- `common-orchestration-runtime-static-graph` 依赖 compiler 输出稳定，并依赖 `RuntimeNodeState` / `StateExchangeSnapshot` 的 materialization 规则。
- `dynamic-script-artifact-compiler` 依赖 common runtime 已经能承载静态 graph，否则会形成平行 scheduler。

## 模块结论

### Orchestration domain contract

第一步应只建立共同 runtime contract 与 `LifecycleRun` 的持久化承载能力。`WorkflowGraphInstance.activity_state`、`ActivityExecutionClaim`、`AgentAssignment`、`RuntimeSessionExecutionAnchor` 仍是当前静态 graph runtime 的迁移来源；新的 `orchestrations[]` 在该阶段先证明 0..N instance 可以被领域模型和 repository 正确保存/读取。

建议文件入口：

- `crates/agentdash-domain/src/workflow/value_objects/orchestration.rs`
- `crates/agentdash-domain/src/workflow/entity.rs`
- `crates/agentdash-domain/src/workflow/repository.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs`
- `crates/agentdash-infrastructure/migrations/0003_lifecycle_orchestration_contract.sql`

最小字段方向：

- domain 字段和本轮新增 PostgreSQL 列都使用 `context`、`orchestrations`、`view_projection`，不带 `_json` / `_jsonb` 后缀；JSON 文本只是存储方式。
- 是否增加 `orchestration_revision` 取决于本阶段是否同步维护 revision 语义。
- `OrchestrationInstance` 至少包含 `orchestration_id`、`role`、`source_ref`、`status`、`plan_snapshot`、`activation`、`node_tree`、`dispatch`、`state_snapshot`、`journal_cursor`、时间戳。

验证闭包：

- `cargo test -p agentdash-domain orchestration`
- `cargo test -p agentdash-infrastructure workflow_repository`
- `pnpm run migration:guard`

### WorkflowGraph compiler

第二步把现有静态 `WorkflowGraph` 当作 definition input，纯函数式编译为 immutable `OrchestrationPlanSnapshot`。编译器不读仓储、不创建 run、不绑定 runtime session，也不做权限授权；它只负责把 graph 的公开语义完整落入 plan IR，并输出可定位的 diagnostics。

建议文件入口：

- `crates/agentdash-domain/src/workflow/orchestration_plan_compiler.rs`
- `crates/agentdash-domain/src/workflow/value_objects/orchestration.rs`
- `crates/agentdash-domain/src/workflow/value_objects/activity_def.rs`
- `crates/agentdash-domain/src/workflow/validation.rs`

必须保留的静态 graph 语义：

- Agent / Function / Human executor identity，其中 API request 与 BashExec 应继续作为 typed function / local effect。
- `completion_policy`、input/output ports、transition condition、artifact binding、`join_policy`、`iteration_policy`、`max_traversals`。
- 当前 runtime 尚未执行的 `Any` / `First` / `NOfM` join、`artifact_alias`、`max_traversals` 也要进入 plan，因为目标 runtime 以 plan 为事实源。

验证闭包：

- graph fixture 编译覆盖 agent create、agent continue、function API、bash、human approval。
- transition fixture 覆盖 all condition variants、default/explicit artifact source、join variants、bounded loop。
- diagnostics 覆盖 dangling refs、unsupported agent policy pair、unbounded cycle、strict artifact mismatch。
- deterministic digest / canonical snapshot roundtrip。

### Common orchestration runtime

第三步把静态 graph 的执行从 `WorkflowGraphInstance.activity_state` 迁移到 `OrchestrationInstance` snapshot / journal 规则。它是架构收敛点：静态 graph 与未来 script compiler 共享同一套 node materialization、dispatch、terminal callback、projection 和 trace anchor。

建议文件入口：

- `crates/agentdash-application/src/workflow/orchestration/`
- `crates/agentdash-application/src/workflow/scheduler.rs`
- `crates/agentdash-application/src/workflow/agent_executor.rs`
- `crates/agentdash-application/src/workflow/orchestrator.rs`
- `crates/agentdash-application/src/workflow/session_association.rs`
- `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs`

状态归属：

- node status、attempt、inputs、outputs、executor refs、error、trace refs 归 `OrchestrationInstance` runtime snapshot / journal materialization。
- claim / lease 是调度并发控制；可以独立表承载热写入，但不能成为业务状态事实源。
- runtime session reverse lookup 是 trace index；目标字段应能解析到 `lifecycle_run_id / orchestration_id / node_path / agent_run_id / frame_id`。
- `LifecycleRunView` 可以先从 orchestration snapshot 生成 graph-compatible projection，支撑现有前端观察模型。

验证闭包：

- static graph 初始化 entry ready node。
- Agent / Function / Human node 分别完成 started -> terminal materialization。
- duplicate terminal event 幂等，不重复推进 successors。
- session terminal callback 经过新 resolver 推进 runtime node。
- view builder 从 orchestration snapshot 继续填充 `workflow_graph_instances` 和 `active_activity_refs`。

## 推荐子任务切分

1. `orchestration-domain-contract`
   - 目标：领域合同、migration、repository roundtrip。
   - 退出条件：`LifecycleRun` 能保存 0..N `OrchestrationInstance`，但旧 runtime 事实源尚未切换。

2. `workflow-graph-compiler`
   - 目标：静态 graph 到 plan snapshot 的 deterministic compiler 和 diagnostics。
   - 退出条件：fixtures 覆盖当前 public graph 语义，并能作为 common runtime 输入。

3. `common-orchestration-runtime-static-graph`
   - 目标：用 plan snapshot 执行静态 graph，并生成旧 view projection。
   - 退出条件：静态 graph e2e 不再依赖 `WorkflowGraphInstance.activity_state` 作为推进事实源。

4. `runtime-trace-anchor-convergence`
   - 目标：把 session terminal / complete tool / cancel control 的反查坐标升级到 orchestration node。
   - 退出条件：runtime session 能稳定反查 lifecycle/orchestration/node/agent/frame，且 terminal callback 幂等。

5. `dynamic-script-artifact-compiler`
   - 目标：引入脚本资产与 script compiler，把动态 workflow 编译到同一 plan IR。
   - 退出条件：动态脚本只新增 compile-time source 与 platform primitives，不新增平行 scheduler。

## 上下文恢复顺序

后续继续本任务时，建议按以下顺序读取：

1. `prd.md`：确认任务目标、文档索引和 gate。
2. `design.md`：确认 Lifecycle / Orchestration 分层、API 命名和阶段设计。
3. `implement.md`：确认子任务列表、验证命令和风险文件。
4. `research/follow-up-module-roadmap.md`：确认后续模块顺序与汇总结论。
5. 三份模块 research：分别恢复 domain contract、compiler、runtime convergence 的源码事实和测试闭包。

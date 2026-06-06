# Orchestration 领域合同实施计划

## 上下文顺序

实现代理必须先读取：

1. 本任务 `prd.md`、`design.md`、`implement.md`。
2. 父任务 roadmap：`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/follow-up-module-roadmap.md`。
3. 父任务模块研究：`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/orchestration-domain-contract-plan.md`。
4. `implement.jsonl` 中列出的当前代码事实和后端 specs。

## 步骤

1. 检查当前 workflow domain exports 和 `LifecycleRun` constructors。
2. 在 domain crate 增加 orchestration value objects。
3. 通过现有 workflow modules re-export 新 value objects。
4. 扩展 `LifecycleRun`：`context`、`orchestrations`、`view_projection`。
5. 添加 aggregate 方法：
   - `set_lifecycle_context`
   - `add_orchestration`
   - `replace_orchestration`
   - `orchestration_by_id`
6. 添加 migration `0003_lifecycle_orchestration_contract.sql`，新增 `context`、`orchestrations`、`view_projection` 三个 JSON 文本承载列。
7. 更新 PostgreSQL `LifecycleRunRepository` row mapping、create、update、select 路径。
8. 添加聚焦测试：
   - plan / node / journal facts 的 domain serde roundtrip；
   - 0..N orchestration instances 的 aggregate 行为；
   - repository row parsing / roundtrip。
9. 运行 targeted checks 并修复本地失败。

## 预计触及文件

- `crates/agentdash-domain/src/workflow/value_objects.rs`
- `crates/agentdash-domain/src/workflow/value_objects/orchestration.rs`
- `crates/agentdash-domain/src/workflow/entity.rs`
- `crates/agentdash-domain/src/workflow/mod.rs`
- `crates/agentdash-infrastructure/migrations/0003_lifecycle_orchestration_contract.sql`
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs`

## 验证命令

```powershell
cargo test -p agentdash-domain orchestration
cargo test -p agentdash-infrastructure workflow_repository
pnpm run migration:guard
git diff --check
```

如果触及代码导致更广的编译失败，先运行最小相关 crate check：

```powershell
cargo check -p agentdash-domain
cargo check -p agentdash-infrastructure
```

## 停止条件

如果实现需要以下内容，停止并报告，不要扩大范围：

- 把 scheduler 或 terminal callback 移到 orchestration state；
- 向 frontend/generated contracts 暴露新的 orchestration DTO；
- 添加 script assets、compiler logic 或 common runtime logic；
- 添加 runtime trace anchor node 坐标列；
- 改变 `WorkflowGraphInstance.activity_state` 语义。

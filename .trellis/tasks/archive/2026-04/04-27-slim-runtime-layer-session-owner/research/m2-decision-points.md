# M2 决策点草案（等 M1-b / M3 完成后与用户确认）

M2 · 状态真相单一化 + state_changes 降级为投影索引（D3-β）。

本 M 项实施前至少需用户明确以下 5 个设计选择。草案列出每个选择的候选与倾向，**不代表已决定**。

---

## D-M2-1 · Task 字段私有化机制

PRD §M2 要求"Task.status / Task.artifacts 降为只读投影"。spec §2.4 已声明"类型级约束必须保证外部不可写"，但未定具体机制。

| 方案 | 描述 | 实施成本 | 对前端 API 影响 | 对 M1-b 返工 |
|---|---|---|---|---|
| A · 私有化 setter | `Task.status` / `artifacts` 字段降为 `pub(crate)`；新增 `Task::apply_projection(&StepState)` 方法；`Story::update_task` 的 mutator closure 收紧为 `FnOnce(&mut TaskSpec)` 仅能改 spec 字段 | 中 | 无（serde 仍序列化所有字段） | 中（M1-b 里 `Story::update_task` closure 的签名需要适配） |
| B · TaskSpec / TaskView 拆分 | `Task` 拆成 `TaskSpec`（durable，可变）+ `TaskView`（spec + 投影字段，只读）；仓储持 `TaskSpec`，API 返回 `TaskView` | 大 | 需要 DTO 映射改动 | 大（Story.tasks 类型从 `Vec<Task>` 改为 `Vec<TaskSpec>`，M1-a/M1-b 代码大面积调整） |
| C · 仅 runtime 层加软锁 | 不改类型，仅在 `Story::update_task` closure 外层 gate "禁止写 status/artifacts"（运行时 panic 或返回 Err） | 低 | 无 | 无 | 

**倾向 A**：ROI 最高，不破坏 M1 已完成代码的类型基线；与 spec §2.4 的 "类型机制" 要求吻合。
**争议点**：如果未来需要把 projection 字段和 spec 字段扩展得相互独立，A 比 B 迁移成本大。但这是 speculative future，本轮不做准备。

---

## D-M2-2 · Projector 注册机制

`state_changes` 作为全局索引由谁写入？spec §6.3 只说"投影器在事件管道中注册"，没定形态。

| 方案 | 描述 | 与现有机制匹配度 |
|---|---|---|
| α · Explicit append in LifecycleRunService | `activate_step` / `complete_step` 内部显式调 `state_change_repo.append`；projector 逻辑就住在 service 里 | 高（现有 `LifecycleRunService` 本就有 repo 依赖） |
| β · Observer / subscriber pattern | 独立的 `LifecycleEventSubscriber` trait；service 发布事件给 subscribers；projector 是其中一个 subscriber | 需新引入 pub-sub 基础设施 |
| γ · Middleware 链 | service 返回 event list，上层 handler 决定如何 process；projector 是 handler 链里的一步 | 改造面大 |

**倾向 α**：最小侵入，与现有 `LifecycleRunService` 风格一致。subscriber/middleware 是"面向未来"的投资，但本轮 P1 痛点是"真相单一"，不是"可扩展事件总线"。

---

## D-M2-3 · Projector 事务边界

`StoryRepository::update(&story)`（一条 UPDATE）与 `state_change_repo::append`（另一条 INSERT）是否同事务？

| 方案 | 描述 | 风险 |
|---|---|---|
| tx-a · 同事务 | 引入事务包裹 `update_story + append_state_change`；跨 repo 方法加 `&mut Transaction` 参数 | `Repository` trait 接口改动面大，需配套改 trait 签名 |
| tx-b · 非事务（现状延续） | 两步 IO 独立提交；若 append 失败 → story 已更新但 state_change 漏了 | 并发/崩溃下短暂裂缝 |
| tx-c · saga / 补偿 | 非事务，但记 outbox；失败时补偿重发 append | 引入 outbox 基础设施，成本最高 |

**M1-a 明确说"non-tx，两步 IO 各自提交"**。**倾向 tx-a 作为主线 M2 目标**——是 M2 真正让"真相单一"落地的关键。但若引入事务对 Repository trait 改动过大，可以先做 tx-b + [UNRESOLVED] 标记供后续提升。

---

## D-M2-4 · ChangeKind 投影归属

现有 9 种 `ChangeKind`（`StoryCreated/Updated/StatusChanged/Deleted` + `TaskCreated/Updated/StatusChanged/Deleted/ArtifactAdded`）中：

| 类别 | 事件 | D3-β 下归属 |
|---|---|---|
| Task runtime 事件 | `TaskStatusChanged`, `TaskArtifactAdded` | **projector 产出**（来自 step state 变更） |
| Task durable 编辑 | `TaskCreated`, `TaskUpdated`, `TaskDeleted` | **业务命令产出**（API 编辑 task spec）→ 走 Story aggregate mutation |
| Story 业务事件 | `StoryCreated`, `StoryUpdated`, `StoryDeleted` | **业务命令产出** |
| Story.status | `StoryStatusChanged` | spec §9 "待澄清 #1"——业务审计字段，**保持命令产出**（不是投影） |

**决策要点**：哪些事件本就是业务命令（API 发起）、哪些是 runtime 投影？草案把 runtime-derived 的（`TaskStatusChanged` / `TaskArtifactAdded`）列为 projector 产出，其余保持命令产出。

**争议**：`TaskUpdated` 目前会在 `task.status = X` 时也触发——这是交织的。M2 要拆清：**status/artifacts 的变更走 runtime projector**；**spec 字段（title/description/agent_binding 等）变更走业务命令**。

---

## D-M2-5 · 启动期 rebuild 策略

`state_reconciler::reconcile_running_tasks_on_boot` 目前是拉式的：启动时扫所有 Running task → 查 session turn state → 投影 status。M1-b 已改为"扫 Story → 扁平化 tasks"。M2 需要把它改成"真正的 event replay"。

| 方案 | 描述 | 复杂度 |
|---|---|---|
| I · 增量投影器 | 启动时按 `state_changes.id` cursor 增量重放；projector 从 cursor 位置继续 | 中 |
| II · 全量扫描 Session event stream | 启动时按 story/task 重新算一遍 status 投影，覆盖当前 Task.status | 低 |
| III · 仅做一次对账 | 启动时扫 Running task 的 session event 确认是否仍活跃，其余依赖 event pipeline 运行时推 | 低（现状延续，只是把数据源换成 session event） |

**倾向 II**：P1 痛点是"状态真相模糊"，启动期全量校准 → 最强"单一真相"保障，成本也可接受。
**风险**：启动时间增加（取决于 story 规模）；对 race condition 敏感（启动时有并发请求可能被覆盖）。

---

## 总结：M2 spawn 前需用户敲定的 5 项

| 决策 | 倾向 | 需用户确认 |
|---|---|---|
| D-M2-1 字段私有化机制 | A 私有化 setter + TaskSpec 内部视图 | ✅ |
| D-M2-2 projector 注册形态 | α service 内 explicit append | ✅ |
| D-M2-3 事务边界 | tx-a 引入事务（激进但根治） | ✅ |
| D-M2-4 ChangeKind 归属 | status/artifacts 走 projector，spec 字段走命令 | ✅ |
| D-M2-5 启动期策略 | II 全量扫描 session event stream | ✅ |

M1-b / M3 完成后汇总这 5 个决策点给用户，一次性对齐后再 spawn M2 agent。

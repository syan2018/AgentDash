# Session 模块完整图景（重构基线）

> 目标：把 `session` 当前“多入口 + 多阶段 + 多次重构叠加”的状态一次性梳理清楚，作为后续所有收敛改造的唯一主线参考。
>
> 范围：`agentdash-application::session` 为核心，补充 `agentdash-api` / `agentdash-local` / task/workflow/routine/companion 对其调用关系。

---

## 1. 先给结论（避免再迷路）

当前系统已经有一条明确主干，并且入口侧已经完成第一轮全源映射：

1. **主干执行内核是统一的**：最终都进入 `SessionHub.start_prompt_with_follow_up()`。
2. **入口表达已统一**：所有生产来源都已挂到 `LaunchIntent`（strict / preparation 两维）。
3. **入口前半段仍分散**：各来源在进入 hub 前的“compose 细节”还未抽成单个 LaunchService。
4. **现阶段最稳约束**：
   - `Init / Continue` 二层语义（公开）
   - `Plain` 禁止误入 owner init compose
- 生产内建入口全部 strict launch（augment 路径 + preassembled 路径）

---

## 2. Session 模块分层

### 2.1 入口层（谁触发）

- HTTP：`agentdash-api/src/routes/acp_sessions.rs`
- Hook auto-resume：`session/turn_processor.rs` -> `hub/hook_dispatch.rs`
- Task：`task/service.rs`
- Workflow：`workflow/orchestrator.rs`
- Routine：`routine/executor.rs`
- Companion dispatch：`companion/tools.rs`
- Companion parent resume：`companion/tools.rs`
- Local relay：`agentdash-local/src/command_handler.rs`

### 2.2 组装层（如何变成可执行请求）

- 类型契约：`session/types.rs`
- 统一组装器：`session/assembler.rs`
- 请求增强接口：`session/augmenter.rs`（API 注入实现）
- owner 增强实现：`api/routes/acp_sessions.rs::augment_prompt_request_for_owner`

### 2.3 执行层（如何真正跑）

- hub 门面：`session/hub/*`
- prompt 启动内核：`session/prompt_pipeline.rs`
- 事件处理：`session/turn_processor.rs`
- hook runtime delegate：`session/hook_delegate.rs`
- 持久化副作用监听：`session/persistence_listener.rs`

---

## 3. 当前“真实入口”总表

| 来源 | 入口函数 | 进入 hub 前行为 | 启动调用 |
|---|---|---|---|
| HTTP prompt | `prompt_session` | base req + owner augment（通过 strict launch） | `launch_http_prompt` |
| Hook auto-resume | `schedule_hook_auto_resume` | 构造 bare req | `launch_hook_auto_resume_prompt` |
| Task start/continue | `StoryStepActivationService::execute_task` | `compose_story_step` + `finalize_request` | `launch_task_prompt` |
| Workflow node kickoff | `LifecycleOrchestrator::start_agent_node_prompt` | `compose_lifecycle_node_with_audit` + `finalize_request` | `launch_workflow_prompt` |
| Routine execute | `RoutineExecutor::execute_with_session` | `build_project_agent_prompt_request`（内部 compose owner） | `launch_routine_prompt` |
| Companion dispatch | `dispatch_companion_request` | `compose_companion(_with_workflow)` + `finalize_request` | `launch_companion_dispatch_prompt_with_follow_up` |
| Companion parent resume | `respond_companion_request` | bare req | `launch_companion_parent_resume_prompt` |
| Local relay prompt | `CommandHandler::handle_prompt` | 本地构造 req（vfs/mcp） | `launch_local_relay_prompt_with_follow_up` |

---

## 4. 启动主线（统一内核）

无论来源，最终都会走到：

1. `start_prompt_with_follow_up(session_id, follow_up, req)`
2. `UserPromptInput.resolve_prompt_payload()`
3. VFS / working_dir / executor config 解析
4. hook runtime load/refresh（根据 `HookSnapshotReloadTrigger`）
5. （可选）hook snapshot injections 合并到 bundle
6. `resolve_session_prompt_lifecycle(...)`
7. 组装 `ExecutionContext` + `SessionFrame/TurnFrame`
8. connector `prompt(...)`
9. `SessionTurnProcessor` 处理通知与终态
10. `before_stop == continue` 时由 processor 请求 hub auto-resume

---

## 5. 生命周期语义（现在应坚持的表达）

### 5.1 公开语义（对重构设计和评审沟通）

- `Init`
- `Continue`

### 5.2 内部执行细分（实现层）

- `SessionPromptLifecycle::OwnerBootstrap` -> `Init`
- `SessionPromptLifecycle::Plain` -> `Continue`
- `SessionPromptLifecycle::RepositoryRehydrate(*)` -> `Continue`（恢复策略）

### 5.3 判定依据（单点）

`resolve_session_prompt_lifecycle(meta, has_live_runtime, supports_repository_restore)`

- `bootstrap_state == Pending` -> `OwnerBootstrap`
- 无 live runtime + 有历史 + 无 executor_follow_up -> `RepositoryRehydrate(...)`
- 否则 `Plain`

---

## 6. strict / 宽松边界（当前状态）

### 6.1 strict（生产内建）

strict 现在分两类：

- **RequiresAugment(strict)**：
  - `launch_http_prompt`
  - `launch_hook_auto_resume_prompt`
  - `launch_companion_parent_resume_prompt`
- **PreAssembled(strict)**：
  - `launch_task_prompt`
  - `launch_workflow_prompt`
  - `launch_routine_prompt`
  - `launch_companion_dispatch_prompt_with_follow_up`
  - `launch_local_relay_prompt_with_follow_up`

行为差异：

- RequiresAugment(strict)：augmenter 缺失/失败 fail-fast，不触发裸请求。
- PreAssembled(strict)：不走 augmenter，直接按已组装请求启动。

### 6.2 宽松（内部保留）

- `launch_prompt_relaxed`：session 模块内部保留（测试/嵌入）
- `augment_prompt_request`：crate 内基础能力（仅供 relaxed 流程内部调用）

---

## 7. 这次问题为何会反复出现（根因模型）

不是“某一行代码写错”这么简单，根因是三类叠加：

1. **入口前半段多实现**  
   各来源都在“进入 hub 前”做了一部分定制，主线概念容易被掩盖。

2. **生命周期语义与执行策略混写**  
   `Init/Continue` 的业务语义，和 `SystemContext/ExecutorState` 的恢复策略，经常被并列描述，导致认知负担飙升。

3. **缺乏全模块总览文档作为真相源**  
   每次都从局部修修补补出发，没人统一核对“这个改动在七条来源里是不是一致”。

---

## 8. 后续收口应遵守的硬不变量（建议直接当 DoD）

1. 所有生产内建来源必须有显式 launch 入口函数（不要裸 `start_prompt`）。
2. `Continue` 禁止进入 owner init compose。
3. `resolve_session_prompt_lifecycle` 仍是唯一 lifecycle 判定点。
4. 任何来源都不得绕过 `finalize_request` 手工拼完整 req。
5. strict 路径下 augmenter 缺失时不得触发 `connector.prompt`。
6. 入口改造必须附带“来源矩阵回归”。

---

## 9. 建议的“彻底收口”执行顺序

### Phase A：入口显式化（先可见）

- 给 Task/Workflow/Routine/Companion/Local 也补齐命名清晰的 launch 入口函数（哪怕内部先转调旧逻辑）。

### Phase B：LaunchIntent 化（先统一表达）

- 把“来源输入 + lifecycle 输入 + owner/binding 输入”收敛为 `LaunchIntent`。

### Phase C：统一 LaunchService（再搬逻辑）

- 所有来源改为 `LaunchService.launch(intent)`。

### Phase D：删旧旁路（最后收口）

- 清除入口层直接 `start_prompt` 的散点调用，保留服务化入口。

---

## 10. 当前阶段状态（截至本次提交）

- 已完成：
  - HTTP/Task/Workflow/Routine/Companion/Local 全来源接入 LaunchIntent 对应入口
  - strict 下 augment 缺失 fail-fast 与回归测试
  - augment 错误语义保真（API error encode/decode）
- 未完成：
  - 将“入口前半段 compose 细节”进一步收敛到单个 LaunchService 实现
  - 形成全来源回归矩阵并固化为 CI 守卫


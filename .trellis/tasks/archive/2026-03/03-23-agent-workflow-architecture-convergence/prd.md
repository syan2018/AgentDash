# Agent / Workflow 架构收口执行计划

## 背景

过去几轮优化已经完成了一批很重要的治理动作：

- `SessionHookSnapshot` 刷新时的关键 metadata 丢失问题已经修复
- 同一 target 并存多个 active workflow run 的行为已经被禁止
- `SessionContextSnapshot` 已经统一为 application 层共享 DTO
- 原先独立的 `workflow_runtime.rs` 已基本退出主链，workflow 注入更多收口到 hook runtime

但从项目长期可持续发展看，系统仍然存在 4 类结构性问题：

1. `WorkflowAssignment` 仍是配置面能力，不是默认执行主线
2. 前端查询用的 session context snapshot 和 Agent 实际 bootstrap context 仍不是同一条 authoritative pipeline
3. workflow binding 的 `document_path / journal_target` 解析仍会绕过 runtime address space，甚至回退到进程 `current_dir`
4. workflow 状态推进、记录产物追加、前端展示语义仍分散在多条入口上

这个任务的目的，不是做一轮局部修补，而是把 Agent / Workflow / Session Context / Hook Runtime 收口成一个长期可维护的架构。

---

## 总体目标

### 目标 1：收口 authoritative source

对下面几类语义，各自只保留一个主来源：

- active workflow runtime projection
- session bootstrap plan
- workflow phase 自动 completion
- 前端可见的 session / workflow runtime snapshot

### 目标 2：让配置能力真正进入业务主线

`WorkflowAssignment` 不应继续停留在“可以配置但不自动生效”的状态，而应正式决定：

- 哪些 owner 在启动 session / task turn 时应自动挂接 workflow
- 如何创建或恢复 active run
- 如何激活当前 phase
- 如何在需要时绑定 session

### 目标 3：让所有运行时解析都依赖 session runtime，而不是进程环境

任何与 workflow / session / context 相关的运行时解析，都应优先依赖：

- address space
- workspace root
- session owner / target
- executor config
- MCP / visible tools

而不是：

- 进程当前目录
- API 路由侧的临时猜测
- 多套逻辑各自重新推导

### 目标 4：让前后端围绕同一份投影工作

前端看到的：

- session context snapshot
- active workflow phase
- tool visibility
- runtime policy
- pending actions / hook trace

必须尽可能来自与 Agent 真正执行相同的 projection / plan，而不是平行复刻。

---

## 非目标

当前阶段明确不做：

- 兼容旧 API 形态的过渡方案
- 为未上线系统保留历史包袱
- 先做 UI 表面统一但保留后端双轨
- 新增更多“解释层抽象”来掩盖已有重复

原则很简单：既然项目还在预研，就优先让结构变正确，而不是变保守。

---

## 目标架构决策

下面这些不是“讨论选项”，而是我建议直接作为本任务的目标状态。

### 决策 A：`WorkflowAssignment` 正式进入默认执行主线

目标状态：

- task execution worker 的 workflow 由 assignment 自动解析
- story owner / project agent 的 workflow 也由 assignment 自动解析
- session 启动时，如果命中 assignment，则自动创建或恢复对应 run
- 如果 phase `requires_session=true`，则在获得 binding 后自动 attach

理由：

- 这能消除“配置了 default assignment 但 runtime 不会自动使用”的语义落差
- assignment 才有资格成为默认编排层，而不是管理后台装饰信息

### 决策 B：自动 completion 的唯一 authority 是 hook runtime

目标状态：

- 自动 phase completion 只允许由 hook runtime 在 post-evaluate 阶段推进
- API route 仍保留手工操作能力，但明确视为人工 override / 管理面动作
- 工具侧允许追加 artifact，但不直接越权完成 phase

理由：

- 自动化推进必须只有一个 writer，否则状态机会继续分散
- route 和 tool 可以保留，但语义要降级为辅助入口，不再争夺默认 authority

### 决策 C：workflow runtime projection 收口为 application 层单一结构

建议新增统一 projection，例如：

```rust
pub struct ActiveWorkflowProjection {
    pub run: WorkflowRun,
    pub definition: WorkflowDefinition,
    pub phase: WorkflowPhaseDefinition,
    pub target: WorkflowTargetSummary,
    pub resolved_bindings: Vec<WorkflowResolvedBinding>,
}
```

所有下面这些消费者都从同一 projection 取数据：

- hook snapshot
- task / story / project bootstrap
- session context snapshot
- frontend workflow runtime 展示

### 决策 D：session bootstrap 收口为统一 plan

建议新增统一 bootstrap plan，例如：

```rust
pub struct SessionBootstrapPlan {
    pub owner: SessionOwnerSummary,
    pub executor: SessionExecutorSummary,
    pub address_space: Option<ExecutionAddressSpace>,
    pub mcp_servers: Vec<McpServer>,
    pub context_fragments: Vec<ContextFragment>,
    pub prompt_blocks: Vec<Value>,
    pub working_dir: Option<String>,
    pub workspace_root: Option<PathBuf>,
    pub runtime_policy: SessionRuntimePolicySummary,
    pub workflow: Option<ActiveWorkflowProjection>,
}
```

目标是让：

- 实际发送给 Agent 的 prompt
- 前端查询到的 snapshot
- hook runtime 看到的 session runtime 元信息

都尽量基于同一 plan 派生，而不是三套 builder 并存。

### 决策 E：workflow binding 必须走 runtime resolver，不允许回退到 `current_dir`

目标状态：

- `document_path` 通过 session address space 或 workspace-root-aware resolver 读取
- `journal_target` 通过同一 resolver 推导落点
- `runtime_context / checklist / action_ref` 保留纯语义型解析
- 严禁 workflow binding resolver 再使用进程 `current_dir()` 兜底

---

## 分阶段执行计划

## Phase 1：统一 runtime projection 和 bootstrap 抽象

### 目标

定义后续所有链路都会依赖的核心 application 结构，先把“说同一种话”的数据模型统一。

### 实现内容

- 新增 `ActiveWorkflowProjection` application service / builder
- 新增 `SessionBootstrapPlan` application service / builder
- 把当前 task / story / project 的 address space、mcp、executor、working_dir、runtime policy 推导逻辑向 application 层收口
- 让 `SessionContextSnapshot` 明确从 bootstrap plan 派生，而不是独立再算一遍

### 涉及文件

- `crates/agentdash-application/src/workflow/*`
- `crates/agentdash-application/src/session_context.rs`
- `crates/agentdash-application/src/context/*`
- `crates/agentdash-api/src/bootstrap/task_execution_gateway.rs`
- `crates/agentdash-api/src/routes/acp_sessions.rs`

### 验收标准

- query path 和 bootstrap path 不再各自独立推导 executor / address space / mcp / runtime policy
- application 层能单独产出一个可测试的 bootstrap plan

---

## Phase 2：让 query snapshot 与实际 bootstrap 共用同一条管线

### 目标

消除“前端看到的 context”和“Agent 真正拿到的 context”继续漂移的风险。

### 实现内容

- `task_execution.rs::build_task_session_context_response` 改为消费统一 plan
- `story_sessions.rs::build_story_session_context_response` 改为消费统一 plan
- `project_sessions.rs::build_project_session_context_response` 改为消费统一 plan
- `acp_sessions.rs` 的 owner bootstrap 也改为消费统一 plan
- 保留 owner-level 差异，但不再保留 owner-specific 重复推导逻辑

### 验收标准

- 同一个 owner/session 的 query 结果与 bootstrap plan 中的 executor / tool visibility / runtime policy / workflow 投影一致
- story/project owner prompt builder 不再拥有独立的 address space / mcp 注入逻辑

---

## Phase 3：把 workflow binding 解析迁移到 runtime resolver

### 目标

让 binding 解析严格依赖当前 session 的运行时环境，停止偷读进程本地文件系统。

### 实现内容

- 重写 `workflow::binding` 的 `document_path` / `journal_target` 解析接口
- resolver 输入改成 runtime-aware context，而不是只拿 `Workspace`
- 优先通过 address space 读取 mount + 相对路径
- 对无 workspace / 无 address space 的情况给出显式 unresolved 结果
- 删除 `current_dir()` fallback

### 验收标准

- workflow binding resolver 不再直接使用 `std::fs::read_to_string` + `current_dir` 做兜底
- project/story/task 三种 target 的解析语义一致
- unresolved 行为是显式的、可诊断的，而不是静默猜测

---

## Phase 4：把 `WorkflowAssignment` 接入默认业务主线

### 目标

让 workflow 配置真正决定运行行为，而不是继续停留在管理面。

### 实现内容

- 新增 assignment resolution service
- 在 task start / story owner prompt / project agent prompt 的 bootstrap 前解析 assignment
- 根据 owner role 选中唯一 default assignment
- 自动创建或恢复对应 run
- 自动激活当前 phase
- 如果 phase 需要 session，则在 binding 就绪后自动 attach
- 保证重复启动时幂等，不制造多余 run

### 关键规则

- 同一 owner + role 只能存在一个默认 assignment
- 同一 target 同时只能存在一个 active run
- `assignment -> run -> phase -> session binding` 必须是稳定可重放的链

### 验收标准

- 配置 default assignment 后，不需要额外手工调用 workflow route 就能进入默认执行链
- 重新进入同一 owner/session 时能恢复到已有 active run，而不是重复创建

---

## Phase 5：收口 workflow 写入 authority

### 目标

让“谁可以推进 phase、谁只能补充 artifact、谁只是人工 override”一眼清楚。

### 实现内容

- 明确 hook runtime 是自动 completion 的唯一 authority
- `report_workflow_artifact` 明确只负责追加 artifact
- workflow routes 标记为人工控制面
- 如果需要人工 complete / activate，保留 route，但要求其行为和 hook progression 共用同一 service
- 为 hook trace / pending action / completion result 增加稳定前端投影

### 验收标准

- 自动推进只发生在 hook runtime 的一条路径
- tool 和 route 都不会再暗中重复承担自动推进职责
- 前端可以明确区分“自动推进结果”和“人工 override”

---

## Phase 6：前端收口、测试与文档补齐

### 目标

避免后端完成收口后，前端仍保留旧的投影模型和解释代码。

### 实现内容

- 前端统一消费新的 session / workflow runtime projection
- 删除过时的重复类型和分支逻辑
- 增加以下测试：
  - workflow assignment 自动接线集成测试
  - bootstrap plan 与 query snapshot 一致性测试
  - binding resolver address space 测试
  - hook completion authority 测试
  - 前端 runtime snapshot 渲染测试
- 更新架构文档，把“现状审查”升级为“现状 + 目标态 + 落地状态”

### 验收标准

- 前后端都围绕同一份 runtime 投影工作
- 新增测试能防止未来再次回到“双轨 builder / 多 authority”状态

---

## 建议的落地顺序

不要并行乱改，建议严格按下面顺序推进：

1. 先做 projection / bootstrap 抽象
2. 再让 query 与 bootstrap 共线
3. 再重写 binding resolver
4. 再把 assignment 接进默认主线
5. 最后收口 authority、前端投影和测试

原因：

- 如果先接 assignment 主线，当前重复 builder 会把复杂度放大
- 如果不先解决 binding resolver，workflow 进入默认主线后只会放大路径语义错误
- 如果不最后统一前端，用户看到的运行时信息会和真实执行继续错位

---

## 关键风险与闸口

### 风险 1：一次性改太宽导致主链不稳

控制方式：

- 每个 Phase 都先收口 application service，再改 API 调用点
- 每阶段结束都跑最小可用集成测试

### 风险 2：assignment 主线化后把 story/project/task 三类 owner 一起复杂化

控制方式：

- 允许 Phase 4 先打通 task execution worker
- story owner / project agent 在同一抽象上逐步接入

### 风险 3：前端继续依赖旧 DTO 导致后端不敢删旧结构

控制方式：

- 前端类型收口必须进入同一任务，而不是留给“以后再整理”

---

## 最终完成定义

当下面这些条件同时满足时，这个任务才算真正完成：

- `WorkflowAssignment` 已进入默认执行主线
- session query snapshot 与实际 bootstrap plan 共线
- workflow binding 不再依赖进程 `current_dir`
- 自动 completion authority 已唯一化
- 前端消费统一 runtime projection
- 关键链路有集成测试覆盖
- 架构文档更新为目标态与现状一致

---

## 本任务与现有任务的关系

本任务是总任务，负责维护全局执行计划。

它会吸收并串联已有成果，例如：

- `03-22-agent-workflow-overdesign-review`
- `03-22-session-context-dedup`

后续如果拆分实施任务，应该以这里定义的目标态为准，避免各个子任务各自优化、最后又重新产生双轨结构。

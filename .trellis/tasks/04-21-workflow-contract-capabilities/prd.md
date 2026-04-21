# capabilities 从 Step 迁移到 Workflow 并统一 MCP 引用语义

## Goal

纠正 workflow 领域模型的结构错位:当前 `LifecycleStepDefinition.capabilities` 承载 agent 单步工具能力声明,而 `WorkflowContract` 反而不含该字段 —— 这与项目既定原则"Workflow 是 agent 单步行为约束,LifecycleStep 是其封装"(见 `memory/workflow_design_principle.md`)完全倒置。同时让 workflow 的 `mcp:<X>` 直接指向 project 级 `McpPreset`,与 `04-20-mcp-preset-agent-binding` 在注入链路上天然统一语义。

## Requirements

### Domain 结构
- `WorkflowContract` 新增 `capabilities: Vec<String>` 字段(纯基线能力 key 集合;Add/Remove 留给 hook runtime 动态授予)
- 删除 `LifecycleStepDefinition.capabilities` 字段
- `compute_effective_capabilities(baseline, directives)` 的调用点改为"baseline 取自 entry step 对应 workflow 的 contract.capabilities"

### 注入链路(核心)
- `CapabilityResolver` 解析 `mcp:<name>` 改为:在 project 的 `McpPresetRepository` 里按 name 查找,展开 `McpServerDecl`
- 新增依赖通过 `CapabilityResolverInput` / `SessionPlanInput` 传入(非全局 singleton)
- agent 侧 `mcp_preset_refs` 展开链路由 `04-20-mcp-preset-agent-binding` 独立完成;本任务只需保证"两条通路并集汇入 effective_mcp_servers"

### Session bootstrap 解析
- `capabilities_from_active_step(step)` → 重命名 `capabilities_from_active_workflow(workflow)`,签名同步替换
- 13 个 working tree 相关调用点(含 resolver / plan_builder / routine executor / orchestrator / advance_node / turn_context / session_runtime_inputs / 三处 session routes / pipeline_tests 等)同步调整
- `session_workflow_context` 的解析链路从"entry step → step.capabilities"改为"entry step → workflow_key → WorkflowDefinition.contract.capabilities"

### Builtin / Migration
- `builtin_workflow_admin.json` 的 capabilities 从 `lifecycle.steps[]` 移到对应 `workflows[].contract.capabilities`(plan / apply 各自的 workflow 独立声明)
- Postgres migration 脚本:扫 lifecycle 表 steps JSON 列,把 step.capabilities 合并到对应 `workflow_key` 所指 workflow 的 contract.capabilities,然后删除 step.capabilities 字段。up/down 幂等
- 冲突策略:同一 workflow 被多 step 引用且声明不一致时,取并集 + `tracing::warn!`

### API / MCP Tool
- `WorkflowContractInput`(MCP `upsert_workflow`)新增 `capabilities: Option<Vec<String>>`
- `StepInput.capabilities`(MCP `upsert_lifecycle`)字段移除(老请求携带该字段时忽略 + warn,不拒绝)
- REST `/api/.../workflows` 相关 DTO 同步

### 前端
- `frontend/src/features/workflow/workflow-editor.tsx` 增加 capabilities 编辑入口:
  - Well-known capability key 多选(file_system / canvas / workflow / collaboration / story_management / task_management / relay_management / workflow_management)
  - 从当前 project 的 MCP Preset 列表里 picker 选择,选中后以 `mcp:<preset_name>` 形式写回 `workflow.contract.capabilities`
- 对应 API client 方法同步

## Acceptance Criteria

- [ ] `WorkflowContract.capabilities: Vec<String>` 字段落地,`LifecycleStepDefinition.capabilities` 完全移除
- [ ] 绑定 `builtin_workflow_admin` 的 agent session 启动后,`effective_mcp_servers` 里存在 `/mcp/workflow/{project_id}`(回归既有链路)
- [ ] Workflow 声明 `capabilities: ["mcp:some_preset"]` 后,session 启动时 `some_preset` 对应的 `McpServerDecl` 被展开加入 `effective_mcp_servers`
- [ ] 迁移脚本对 `builtin_workflow_admin` 的历史记录跑完后,workflow.contract.capabilities 包含 `workflow_management`,step.capabilities 字段消失
- [ ] `capabilities_from_active_workflow` 改名完成,所有 13 个调用点编译通过
- [ ] 前端 workflow-editor 可编辑 capabilities 并保存,刷新后回显一致
- [ ] `cargo clippy` / `cargo test` / `tsc` / eslint 全绿

## Definition of Done

- Happy path 单测:WorkflowContract.capabilities 解析、`mcp:<X>` 查 Preset、migration 脚本 roundtrip
- 一条 session_workflow_context → resolver → effective_mcp_servers 的集成测试
- Postgres migration 干净 up/down,已有 lifecycle 记录迁移后保持能用
- 更新 `memory/workflow_design_principle.md` 追加一行 "capabilities 归属 WorkflowContract,step 不承担能力声明"

## Technical Approach

### 解析顺序(新)
1. `resolve_session_workflow_context` → 找到 entry step → 取 `step.workflow_key`
2. 查 `WorkflowDefinitionRepository.get_by_project_and_key` → 取 `contract.capabilities`
3. `capabilities_from_active_workflow(workflow)` 返回 baseline `Vec<String>`
4. Resolver 对每个 key:
   - well-known → 映射 ToolCluster / PlatformMcpScope
   - `mcp:<name>` → 查 `McpPresetRepository` → 展开 server_decl 加入 `custom_mcp_servers`

### 两条通路合并
- Agent `mcp_preset_refs` 展开(由 `04-20-mcp-preset-agent-binding` 交付):在调用 `SessionPlanBuilder::build` 前由 route 层展开
- Workflow `mcp:<X>` 解析(本任务):resolver 内部完成
- 两者最终都写入 `SessionPlanOutput.effective_mcp_servers`,按 server name 去重

## Decisions (ADR-lite)

### D1 — `LifecycleStepDefinition.capabilities` 完全删除
- **Context**: 设计原则倒置;担心的"一个 workflow 被多 step 共享时需要差异化"场景不成立 —— builtin_workflow_admin 的 plan / apply 两 step 分别指向 `builtin_workflow_admin_plan` / `builtin_workflow_admin_apply` 两个独立 workflow
- **Decision**: 删除字段,capabilities 只在 workflow.contract 声明
- **Consequences**: 未来如需 step 级差异化被迫拆 workflow,这是正确的设计压力

### D2 — `mcp:<X>` 指向 project 级 McpPreset
- **Context**: 原语义按 agent_mcp_servers name 匹配,与 mcp-preset-agent-binding 统一不起来
- **Decision**: 复用 `mcp:` 前缀,`<X>` 语义改为 project 级 Preset name
- **Consequences**: capability key 变成 project-scoped,不再依赖 agent 配置形状;agent 侧的 inline mcp_servers 逐步被 preset_refs 取代

### D3 — workflow 自治,不依赖 agent 预声明
- **Context**: 权限边界取舍
- **Decision**: resolver 直接查 Preset,不经 agent_mcp_servers 中转
- **Consequences**: 两条 MCP 注入通路并列独立,通过"共用 McpPresetRepository"隐式统一语义;与 mcp-preset-agent-binding 解耦

### D4 — 自动 SQL 迁移脚本
- **Decision**: 给 lifecycle 表 steps JSON 列写一次性迁移,合并 step.capabilities 到 workflow.contract.capabilities 并清理 step 字段;冲突时取并集 + warn

## Out of Scope

- Hook runtime 动态能力授予链路(由 `04-20-dynamic-capability-followup` 独立交付)
- Session workflow 上下文装配的其他接线(由 `04-20-session-workflow-context-wiring` 独立交付)
- Agent 配置侧的 `mcp_preset_refs` 字段本身(由 `04-20-mcp-preset-agent-binding` 交付)
- Preset Bundle / 跨 project 引用 / Preset 版本锁定(`04-20-unified-assets-page` 已声明 OOS)
- capability key 非 MCP 场景(如 `canvas:<mount_id>`),本任务只处理 `mcp:*` + well-known
- Edge case 全覆盖单测(Preset 被删 / name 不匹配 / 迁移冲突等只在 happy path 基础上按需补,不强制)
- Trellis builtin(`trellis_dag_task` 等)迁移,计划删除

## Implementation Plan

- **PR1 — Domain + Migration 骨架**
  - `WorkflowContract.capabilities` 字段
  - `LifecycleStepDefinition.capabilities` 删除
  - `compute_effective_capabilities` 调整为读 workflow baseline
  - Postgres migration 脚本(up/down + 冲突并集策略)
  - `builtin_workflow_admin.json` 重写
  - Happy path 单测
- **PR2 — Resolver + 注入链路 + helper 改名**
  - `CapabilityResolver` 解析 `mcp:<X>` 查 `McpPresetRepository`
  - `CapabilityResolverInput` 新增 preset repo 依赖(或预展开输入)
  - `capabilities_from_active_step` → `capabilities_from_active_workflow` 改名 + 13 个调用点同步
  - `session_workflow_context` 解析链路改为 workflow_key → WorkflowDefinition.contract
  - 集成测试(session bootstrap → effective_mcp_servers)
- **PR3 — MCP tool schema + 前端编辑入口**
  - `WorkflowContractInput.capabilities` 新增
  - `StepInput.capabilities` 移除(兼容 warn)
  - `workflow-editor.tsx` 加 capabilities 编辑区(well-known 多选 + Preset picker)
  - API client / DTO 同步

## Technical Notes

### 依赖与阻塞
- **依赖交付**:`McpPreset` domain + `McpPresetRepository`(`04-20-unified-assets-page` 已基本落地)
- **关联不阻塞**:`04-20-mcp-preset-agent-binding`(并列开发,共享 repository 接口)

### 关键文件
- domain: `crates/agentdash-domain/src/workflow/value_objects.rs` / `entity.rs`
- application: `crates/agentdash-application/src/capability/{resolver,session_workflow_context}.rs` `session/plan_builder.rs` `workflow/{orchestrator,tools/advance_node}.rs` `routine/executor.rs` `task/{gateway/turn_context,session_runtime_inputs}.rs`
- api: `crates/agentdash-api/src/routes/{project_sessions,story_sessions,acp_sessions}.rs`
- mcp: `crates/agentdash-mcp/src/servers/workflow.rs`
- builtin: `crates/agentdash-application/src/workflow/builtins/builtin_workflow_admin.json`
- frontend: `frontend/src/features/workflow/workflow-editor.tsx`

### 风险点
- 自动迁移脚本的并集策略若不对齐,在 user 自定义 lifecycle 场景可能出现能力膨胀 —— 保留 warn 给人工复核
- `CapabilityResolver` 从纯函数改为需要 repo 依赖,测试 mock 增加 —— 可考虑调用方预展开(把 Preset 查询提到 SessionPlanBuilder 里)以保持 resolver 纯函数性

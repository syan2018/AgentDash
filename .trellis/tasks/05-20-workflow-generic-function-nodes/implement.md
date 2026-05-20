# 工作流通用功能节点扩展实施计划

## 0. 准备

- [ ] 读取编码前规范：
  - `.trellis/spec/backend/index.md`
  - `.trellis/spec/backend/workflow/lifecycle-edge.md`
  - `.trellis/spec/backend/runtime-gateway.md`
  - `.trellis/spec/cross-layer/desktop-local-runtime.md`
  - `.trellis/spec/frontend/index.md`
  - `.trellis/spec/shared/index.md`
- [ ] 确认是否需要数据库迁移：检查 `lifecycle_definitions.steps`、`lifecycle_runs.execution_log` 的 schema 是否有 CHECK / enum 限制。
- [ ] 确认 Bash 执行复用路径：优先复用 VFS exec / relay shell_exec / local ToolExecutor 中现有能力。

## 1. 领域模型

- [ ] 在 `crates/agentdash-domain/src/workflow/value_objects.rs` 扩展 `LifecycleNodeType::FunctionNode`。
- [ ] 增加 `FunctionNodeSpec`、`ApiRequestNodeSpec`、`BashExecNodeSpec` 及相关 output mapping 类型。
- [ ] 在 `LifecycleStepDefinition` 增加 `function: Option<FunctionNodeSpec>`。
- [ ] 扩展 `validate_lifecycle_definition`：
  - `function_node` 必须有 function spec。
  - `agent_node` / `phase_node` 不携带 function spec。
  - `function_node` 不绑定 workflow_key。
  - entry step 首版仍只能是 `agent_node`。
- [ ] 增加 serde roundtrip 和校验单元测试。

## 2. 应用层执行器

- [ ] 新建 workflow function executor 模块，例如 `crates/agentdash-application/src/workflow/function_node/`。
- [ ] 定义 `FunctionNodeExecutor` 入口：
  - 输入：run、lifecycle、step、input artifacts、workspace routing、timeout。
  - 输出：status、summary、mapped artifacts、log detail。
- [ ] 实现 API Request executor：
  - method/url/header/query/body 组装。
  - timeout 和响应大小限制。
  - status/body/headers 输出映射。
  - header 脱敏写日志。
- [ ] 实现 Bash Exec executor：
  - 解析 workspace/backend/cwd。
  - 调用现有本机执行能力。
  - 收集 stdout/stderr/exit_code/duration。
  - timeout 和输出截断。
- [ ] 增加 application 层测试，覆盖成功、失败、timeout、output mapping。

## 3. Orchestrator 接入

- [ ] 在 `LifecycleOrchestrator::activate_ready_nodes` 增加 FunctionNode 分支。
- [ ] 执行前调用 `activate_step`。
- [ ] 成功后写 port artifacts，追加 execution log，调用 `complete_step`。
- [ ] 失败后追加 execution log，调用 `fail_step`。
- [ ] 支持 function node 完成后继续触发后继 activation。
- [ ] 增加推进上限或异步调度边界，避免长链 function nodes 在单个请求中无限推进。
- [ ] 增加 orchestrator 测试：
  - function node 成功推进后继。
  - function node 失败阻断后继。
  - function output artifact 可被后继 artifact edge 识别。

## 4. API / DTO / 持久化

- [ ] 同步 `crates/agentdash-api/src/dto/workflow.rs`。
- [ ] 检查 routes 的 create/update/validate lifecycle 是否需要显式处理 function node。
- [ ] 检查 postgres repository 的 JSON 序列化是否无需修改。
- [ ] 如存在 schema 约束，新增 migration 并更新测试。

## 5. 前端类型与状态

- [ ] 更新 `packages/app-web/src/types/workflow.ts` 的 node type 和 function spec 类型。
- [ ] 更新 `workflowStore.ts`：
  - function node 初始化。
  - node type 切换清理不适用字段。
  - function node 保存时跳过 workflow draft upsert。
  - lifecycle validate/save payload 正确。
- [ ] 更新 `shared-labels.ts` 和运行状态 label。
- [ ] 更新相关 store 测试。

## 6. 前端 UI

- [ ] 更新 `dag-node.tsx`：Function Node 标签、视觉样式、端口展示。
- [ ] 更新 `step-inspector.tsx`：
  - node type 支持 Function。
  - Function 节点展示专属配置面板。
  - Agent / Phase 保持现有 workflow contract 面板。
- [ ] 新增 Function 配置面板组件：
  - API Request panel。
  - Bash Exec panel。
  - Output Mapping panel。
- [ ] 更新 `lifecycle-session-view.tsx` 展示 function execution summary / error。
- [ ] 增加前端测试，覆盖编辑、切换、保存 payload。

## 7. Builtin / 示例

- [ ] 视情况新增一个内置 lifecycle 示例：
  - Agent plan → API request → Agent analyze。
  - Agent plan → Bash exec → Agent check。
- [ ] 确保 builtin JSON 显式包含 edge kind 和 function spec。

## 8. 验证命令

- [ ] Rust domain/application 相关测试：
  - `cargo test -p agentdash-domain workflow`
  - `cargo test -p agentdash-application workflow`
  - `cargo test -p agentdash-api workflow`
- [ ] 前端测试：
  - `pnpm test -- workflow`
  - `pnpm typecheck`
- [ ] 如涉及迁移：
  - 运行 SQLx / migration 相关验证命令。
- [ ] 手动验证：
  - `pnpm dev`
  - 创建带 API Request function node 的 lifecycle，执行后确认 artifact 和后继推进。
  - 创建带 Bash Exec function node 的 lifecycle，确认本机 backend 执行、cwd 正确、输出写入 artifact。

## 9. 回滚点

- [ ] 领域模型变更前：确认 `LifecycleStepDefinition` serde 测试。
- [ ] Orchestrator 接入前：Function executor 可单测独立通过。
- [ ] 前端保存变更前：后端 validate endpoint 已能接受 function node。
- [ ] 最终合并前：确认不需要兼容旧 function schema；预研期直接保留最正确结构。


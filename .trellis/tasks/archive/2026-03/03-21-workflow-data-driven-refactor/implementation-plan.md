# Workflow 数据驱动重构实施方案

## 重构目标

把当前 Workflow 从“Trellis 特化逻辑包一层通用外壳”，收敛成“全局 builtin template + definition/run 运行态”的平台结构。

## 本轮已落地

### 1. 内置 Workflow 模板数据化

- 新增 `crates/agentdash-application/src/workflow/builtins/*.json`
- 由 JSON 模板定义：
  - `key`
  - `name`
  - `description`
  - `target_kind`
  - `recommended_role`
  - `phases`
  - `record_policy`
- `definition.rs` 不再手写 Trellis phase，而是负责加载 / 校验 / 构建 definition

### 2. API 收敛为通用模板机制

- 新增 `GET /api/workflow-templates`
- 新增 `POST /api/workflow-templates/{builtin_key}/bootstrap`
- 去掉 Trellis 专用 bootstrap 入口

### 3. 运行时关键约束补齐

- `start_workflow_run` 先校验 target 是否存在
- `activate/complete phase` 增加 session binding 与 workflow target 的归属校验

### 4. 前端去 Trellis 特化

- Project 面板改为：
  - 先展示 builtin templates
  - 再展示已注册 definitions
- 绑定默认流程时，不再写死 Trellis 按钮和 target 列表
- Task 面板生成阶段 artifact 时，不再依赖 `phaseKey === "record"` 特判，而是读取 phase definition 自带的 artifact 默认配置

### 5. Phase 定义进一步结构化

- `context_bindings` 从单一字符串路径升级为：
  - `kind`
  - `locator`
  - `reason`
  - `required`
  - `title`
- 前端开始按 binding kind 和 completion mode 解释 phase，而不是只把它当静态文本显示

### 6. Workflow Runtime 注入层已接入真实会话链路

- 新增 `crates/agentdash-api/src/workflow_runtime.rs`
- 现在会按 `workflow run -> current phase -> bindings/instructions` 解析出运行时注入结果
- 注入结果不再只停留在 UI 展示，而会实际进入：
  - Task 执行 prompt/context 构建链路
  - Story owner session prompt blocks
  - Project owner session prompt blocks
- `WorkflowPhaseDefinition` 新增 `agent_instructions`
  - 由模板数据定义 phase 级约束
  - 运行时自动注入给 Agent

### 7. 前端去掉“只认 Task Trellis 流程”的残余耦合

- Project Workflow 面板改为按 `role` 分组展示 definition
- 不再只支持 `task_execution_worker`
- phase 卡片开始展示：
  - 自动注入约束（`agent_instructions`）
  - binding 列表
  - completion mode
- Task Workflow 面板现在会明确显示“当前 phase 会自动注入给 Agent 的阶段约束”

## 当前结构

### 全局模板层

- 来源：仓库内 JSON builtin templates
- 职责：提供全局可注册的 workflow 定义模板

### Definition 层

- 来源：由 template bootstrap 后写入存储
- 职责：提供 Project assignment 与 run 创建所依赖的正式 definition

### Assignment 层

- 职责：把 definition 绑定到 Project + role

### Run 层

- 职责：维护 target 上的 phase 运行态、记录产物与 session binding 关系

## 下一步建议

### 优先级 P1

- 为 workflow template 增加更明确的 source metadata
- 让前端支持按 role 过滤和绑定，不只支持 task_execution_worker
- 为 target 校验 / binding 校验补充 API 测试

### 优先级 P2

- 为 `completion_mode` 增加真正的数据驱动执行语义，而不仅是展示和提示字段
- 区分 builtin definition 与用户自定义 definition 的展示和管理方式
- 把当前 `workflow_runtime` 的 builtin locator resolver 下沉到更稳定的 application/runtime 层
- 继续对齐“真实 Trellis hooks”语义：
  - SessionStart / PreToolUse 风格的自动注入
  - task 目录 jsonl 与 phase 绑定
  - 基于当前 task/status 的动态上下文切换

### 优先级 P3

- 设计更通用的 workflow designer
- 评估是否需要数据库持久化 template registry 或版本追踪

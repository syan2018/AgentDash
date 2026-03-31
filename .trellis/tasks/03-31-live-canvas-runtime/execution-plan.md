# Live Canvas — 执行计划

> 目标：把 Live Canvas 从“已收敛的方案设计”推进到“可分片并行实施的工程任务”。
> 当前分支：`codex/live-canvas-runtime`
> 基线提交：`c87575e`（方案与落地计划文档）

---

## 总体策略

这项工作跨越：

- 后端领域模型 / 持久化
- Address Space / Runtime Tool
- ACP Session 系统事件
- 前端 SessionPage 容器
- 浏览器内 Canvas Runtime

因此不适合按“前端 / 后端”两大块粗切，而更适合按**收敛边界**拆成 5 个工作包：

1. `canvas-domain-api`
2. `canvas-address-space-tools`
3. `canvas-runtime-contract`
4. `session-page-canvas-panel`
5. `canvas-samples-and-authoring-docs`

其中 1 和 3 可以先并行，2 依赖 1 的实体与 API 轮廓，4 依赖 3 的接口契约，5 最后收尾。

---

## 共享前提

所有子任务默认基于以下事实：

- Canvas 是 **Project 级独立资产**，不是 `context_containers`
- Canvas mount 进入 session address space 的方式，参考 lifecycle mount：**基础派生 + 运行时追加**
- 文件编辑继续复用现有 `fs_read` / `fs_write` / `fs_apply_patch`
- Canvas 运行时加载走 **独立 runtime snapshot API**
- 前端首版只在独立 [SessionPage.tsx](/F:/Projects/AgentDash/frontend/src/pages/SessionPage.tsx) 落地 Canvas Panel
- Runtime 首版是自研 `iframe sandbox`，不把 Sandpack 当正式内核

---

## 工作包 1：`canvas-domain-api`

### 目标

- 建立 Canvas 的领域实体、仓储接口、应用服务和 HTTP API
- 为后续 mount provider / runtime snapshot / present 工具提供 authoritative 数据模型

### 主要产出

- Canvas 实体定义
- CanvasBinding / 数据源绑定定义
- repository trait + SQLite 实现
- API DTO 和 CRUD / snapshot / present 所需路由骨架

### 关键文件

- `crates/agentdash-domain/`
- `crates/agentdash-application/`
- `crates/agentdash-infrastructure/src/persistence/sqlite/`
- `crates/agentdash-api/src/routes/`

### 完成标准

- 能按 `project_id` 创建 / 查询 / 更新 Canvas
- 能持久化 `entry_file`、`sandbox_config`、`bindings`
- 能为 runtime snapshot service 提供稳定读取接口

### 依赖

- 无

---

## 工作包 2：`canvas-address-space-tools`

### 目标

- 让 Canvas 作为 session 可见 mount 进入统一 Address Space
- 注入最小 Canvas 工具能力，同时继续复用现有 `fs_*`

### 主要产出

- `PROVIDER_CANVAS_FS`
- `canvas_fs` provider
- Address Space 追加 Canvas mount 的 authoritative pipeline
- `FlowCapabilities` 扩展
- `create_canvas` / `inject_canvas_data` / `present_canvas` 最小工具

### 关键文件

- [mount.rs](/F:/Projects/AgentDash/crates/agentdash-application/src/address_space/mount.rs)
- [provider.rs](/F:/Projects/AgentDash/crates/agentdash-application/src/address_space/provider.rs)
- [relay_service.rs](/F:/Projects/AgentDash/crates/agentdash-application/src/address_space/relay_service.rs)
- [tools/provider.rs](/F:/Projects/AgentDash/crates/agentdash-application/src/address_space/tools/provider.rs)
- [connector.rs](/F:/Projects/AgentDash/crates/agentdash-spi/src/connector.rs)
- [acp_sessions.rs](/F:/Projects/AgentDash/crates/agentdash-api/src/routes/acp_sessions.rs)
- [session_plan.rs](/F:/Projects/AgentDash/crates/agentdash-application/src/session_plan.rs)
- [task/session_runtime_inputs.rs](/F:/Projects/AgentDash/crates/agentdash-application/src/task/session_runtime_inputs.rs)

### 完成标准

- 指定 session 能看到 Canvas mount
- Agent 可以通过 `fs_write(canvas-xxx://...)` 修改 Canvas 文件
- Canvas 工具的真实注入与 prompt 摘要一致

### 依赖

- 依赖 `canvas-domain-api` 的实体 / repo 基础

---

## 工作包 3：`canvas-runtime-contract`

### 目标

- 定义前后端共享的 runtime snapshot 契约
- 明确 iframe runtime 的输入 / 输出 / 错误协议

### 主要产出

- runtime snapshot API 设计
- snapshot payload 字段定义
- iframe `postMessage` 协议
- 受控库白名单 / import map 方案
- `esbuild-wasm` 接入边界

### 关键文件

- [prd.md](/F:/Projects/AgentDash/.trellis/tasks/03-31-live-canvas-runtime/prd.md)
- [frontend/package.json](/F:/Projects/AgentDash/frontend/package.json)
- [SessionPage.tsx](/F:/Projects/AgentDash/frontend/src/pages/SessionPage.tsx)
- [agentdash-acp-meta/lib.rs](/F:/Projects/AgentDash/crates/agentdash-acp-meta/src/lib.rs)

### 完成标准

- 明确 `GET /api/canvases/{id}/runtime-snapshot?session_id=...` 的输入输出
- 明确 iframe 启动消息、刷新消息、错误消息
- 明确首版支持的文件类型与依赖策略

### 依赖

- 可与 `canvas-domain-api` 并行启动

---

## 工作包 4：`session-page-canvas-panel`

### 目标

- 在 SessionPage 内引入页面级 Canvas Panel
- 基于 ACP 系统事件打开指定 Canvas，并拉取 snapshot 渲染

### 主要产出

- `canvas_presented` 事件展示与消费
- `SessionPage` Canvas 状态管理
- Canvas Panel UI
- 加载态 / 空态 / 错误态

### 关键文件

- [SessionPage.tsx](/F:/Projects/AgentDash/frontend/src/pages/SessionPage.tsx)
- [AcpSystemEventGuard.ts](/F:/Projects/AgentDash/frontend/src/features/acp-session/ui/AcpSystemEventGuard.ts)
- [AcpSystemEventCard.tsx](/F:/Projects/AgentDash/frontend/src/features/acp-session/ui/AcpSystemEventCard.tsx)
- [SessionChatView.tsx](/F:/Projects/AgentDash/frontend/src/features/acp-session/ui/SessionChatView.tsx)

### 完成标准

- 收到 `canvas_presented` 后可打开面板
- 面板能加载指定 Canvas 并显示运行状态
- 不破坏当前 SessionChatView 流程

### 依赖

- 依赖 `canvas-runtime-contract`

---

## 工作包 5：`canvas-samples-and-authoring-docs`

### 目标

- 给 Agent 和低代码同学形成可复用样例与约定
- 减少首版使用过程中的试错成本

### 主要产出

- 示例 Canvas：表格 / ECharts 图表
- 目录约定和入口约定
- 数据绑定别名约定
- 常见错误说明

### 关键文件

- `.trellis/tasks/03-31-live-canvas-runtime/`
- `docs/` 或 `.trellis/spec/` 中的补充说明

### 完成标准

- Agent 能按固定模板生成可运行 Canvas
- 至少有 2 个示例可验证主链路

### 依赖

- 依赖前 4 个工作包完成最小闭环

---

## 建议并行顺序

### 第一波并行

- `canvas-domain-api`
- `canvas-runtime-contract`
- `session-page-canvas-panel` 只做现状摸底与接入点确认，不直接写实现

### 第二波并行

- `canvas-address-space-tools`
- `session-page-canvas-panel`

### 第三波

- `canvas-samples-and-authoring-docs`

---

## 风险提示

### 风险 1：Canvas 资产模型被塞回 `context_containers`

后果：

- 资产生命周期和上下文容器语义混淆
- mount 可见性与 Project/Story 配置强耦合
- 后续 Canvas CRUD / 版本化 / 面板展示都难做

### 风险 2：系统事件直接携带大块文件内容

后果：

- ACP 流噪声变大
- 历史会话负担变重
- 前端重连和恢复复杂度上升

### 风险 3：前端过早追求 HMR / 通用 bundler

后果：

- 实现面显著膨胀
- 首版可交付时间被拖长

---

## 第一批建议负责人

- 主线程：统一收口、接口决策、跨包集成
- Subagent A：`canvas-domain-api`
- Subagent B：`canvas-runtime-contract`
- Subagent C：`session-page-canvas-panel`


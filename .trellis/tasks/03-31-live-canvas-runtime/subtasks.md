# Live Canvas 可执行子任务

## 1. `canvas-domain-schema-and-repo`

### 目标

- 新增 Canvas 实体与 bindings 模型

### 产出

- domain types
- repository trait
- SQLite schema / repo

### 涉及文件

- `crates/agentdash-domain/`
- `crates/agentdash-infrastructure/src/persistence/sqlite/`

---

## 2. `canvas-application-service-and-api`

### 目标

- 提供 Canvas CRUD、present、runtime snapshot 所需应用服务与 API

### 涉及文件

- `crates/agentdash-application/`
- `crates/agentdash-api/src/routes/`

---

## 3. `canvas-provider-and-session-mount`

### 目标

- 实现 `canvas_fs` provider，并让 Canvas mount 进入 session address space

### 涉及文件

- `crates/agentdash-application/src/address_space/`
- `crates/agentdash-application/src/task/session_runtime_inputs.rs`
- `crates/agentdash-api/src/routes/acp_sessions.rs`

---

## 4. `canvas-runtime-tools-and-capabilities`

### 目标

- 扩展 `FlowCapabilities`，注入最小 Canvas 工具

### 涉及文件

- `crates/agentdash-spi/src/connector.rs`
- `crates/agentdash-application/src/address_space/tools/provider.rs`
- `crates/agentdash-application/src/session_plan.rs`

---

## 5. `canvas-runtime-snapshot-contract`

### 目标

- 明确 runtime snapshot payload 与 iframe message 协议

### 涉及文件

- `.trellis/tasks/03-31-live-canvas-runtime/prd.md`
- `frontend/package.json`

---

## 6. `session-page-canvas-panel`

### 目标

- 在 SessionPage 打开并展示 Canvas

### 涉及文件

- `frontend/src/pages/SessionPage.tsx`
- `frontend/src/features/acp-session/ui/`

---

## 7. `iframe-runtime-loader`

### 目标

- 实现 iframe bootstrap、受控依赖加载、错误桥接

### 涉及文件

- `frontend/src/features/`
- `frontend/package.json`

---

## 8. `canvas-samples-and-agent-authoring-guide`

### 目标

- 样例 + 约定 + 使用文档

### 涉及文件

- `.trellis/tasks/03-31-live-canvas-runtime/`
- `docs/` / `.trellis/spec/`

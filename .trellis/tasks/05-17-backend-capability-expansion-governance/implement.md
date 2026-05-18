# 实施计划：Backend 能力扩展治理设计

## 阶段 0：规划收口

- [ ] Review `prd.md` / `design.md`，确认 control mode、relay/ack、personal 确认、TTL、worktree/prepare 复用请求模型这五个决策。
- [ ] 补齐 `implement.jsonl` / `check.jsonl` 的 spec/research manifest。
- [ ] 用户确认后再 `task.py start`，进入实现。

## 阶段 1：领域模型与持久化

- [ ] 在 `agentdash-domain::backend` 新增：
  - `BackendControlMode`
  - `BackendCapabilityExpansionRequest`
  - request status / actor kind / source kind / target scope kind 枚举
  - typed policy / requested resource helper（至少提供 validator，不让 route 裸拼 JSON）
- [ ] 给 `BackendConfig` 增加 `control_mode`。
- [ ] 更新 `BackendRepository` / `PostgresBackendRepository`，确保 `ensure_local_backend` 写入 control mode。
- [ ] 新增 `BackendCapabilityExpansionRequestRepository`。
- [ ] 在 Postgres 初始化中新增 `backend_capability_expansion_requests` 表和索引。
- [ ] 补 repository 单测或 Postgres round-trip 测试。

验证命令：

```powershell
cargo test -p agentdash-domain backend
cargo test -p agentdash-infrastructure backend_capability
cargo check -p agentdash-api
```

## 阶段 2：Server API 与 policy 裁决

- [ ] 新增 application service，例如 `backend_capability_expansion`：
  - 创建请求
  - 按 `control_mode` 和 Project/admin 权限做 policy decision
  - approve / reject / revoke / retry
  - 过滤 active/applied requests
- [ ] 新增 API route：
  - `POST /projects/{project_id}/backend-access/{access_id}/capability-requests`
  - `GET /projects/{project_id}/backend-access/{access_id}/capability-requests`
  - `GET /backend-capability-requests`
  - `POST /backend-capability-requests/{id}/approve|reject|revoke|retry`
- [ ] Project 范围创建必须校验：
  - current user 可 edit Project
  - access 属于 Project
  - access status 为 active
  - target backend 与 access.backend_id 一致
- [ ] admin/system managed 批量入口可以先做后续阶段；首版先支持单 backend 请求。

验证命令：

```powershell
cargo test -p agentdash-api backend_capability
cargo test -p agentdash-application backend_capability
pnpm run backend:check
```

## 阶段 3：Relay apply / ack 协议

- [ ] 在 `agentdash-relay` 增加：
  - `command.capability_expansion_apply`
  - `response.capability_expansion_apply`
  - 可选 `command.capability_expansion_revoke`
- [ ] 在 server relay registry / handler 中增加在线下发与 response 解析。
- [ ] 在 local backend handler 中实现最小处理：
  - 已在 accessible roots 内的请求可直接 ack applied。
  - personal 新增任意 root 返回需要本机确认的 rejected/pending reason，或进入本地 pending 队列。
  - managed backend 可按 profile/policy 自动接受。
- [ ] ack 成功后更新 request `backend_ack` / `status`。
- [ ] ack 后触发或提示 inventory register/refresh；不要在 ack 前伪造 runtime health。

验证命令：

```powershell
cargo test -p agentdash-relay capability_expansion
cargo test -p agentdash-api relay
cargo test -p agentdash-local capability_expansion
```

## 阶段 4：Tauri Local Runtime 确认面

- [ ] 在 Tauri command / local runtime manager 中暴露 pending expansion requests。
- [ ] Local Runtime 面板新增 pending requests 区块：
  - request 来源 project/workspace/task
  - requested roots/capabilities
  - approve / reject
- [ ] approve 后更新本机 profile / runtime accessible roots，再 ack server。
- [ ] reject 必须写明中文可读原因。

验证命令：

```powershell
pnpm run desktop:check
pnpm --filter app-tauri typecheck
cargo check -p agentdash-local-tauri
```

## 阶段 5：Server 前端设置入口

- [ ] 扩展 `BackendConfig` / API types，加入 `control_mode`。
- [ ] 新增 backend capability request types/service。
- [ ] Project Backend Access 面板增加“请求扩展能力”入口和状态列表。
- [ ] Backend 设置/detail 增加 request 列表、ack/reject/retry/revoke 状态展示。
- [ ] UI 必须区分：
  - server accepted
  - waiting backend ack
  - applied
  - rejected/failed/revoked/expired
- [ ] 不把 `accepted` 展示成“已生效”。

验证命令：

```powershell
pnpm --filter app-web typecheck
pnpm --filter app-web lint
pnpm --filter app-web test
```

## 阶段 6：跨层收尾

- [ ] 更新 `.trellis/spec/cross-layer/project-backend-workspace-routing.md`：说明 capability expansion 只提供已 ack 能力，不替代 ProjectBackendAccess。
- [ ] 如新增 relay protocol，更新相关 cross-layer spec。
- [ ] 运行：

```powershell
pnpm run backend:check
pnpm run frontend:check
pnpm run frontend:test
```

## 风险文件

- `crates/agentdash-domain/src/backend/entity.rs`
- `crates/agentdash-domain/src/backend/repository.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/project_backend_access_repository.rs`
- `crates/agentdash-api/src/routes/backend_access.rs`
- `crates/agentdash-api/src/routes/backends.rs`
- `crates/agentdash-api/src/relay/registry.rs`
- `crates/agentdash-relay/src/protocol.rs`
- `crates/agentdash-local/src/handlers/`
- `crates/agentdash-local-tauri/src/main.rs`
- `packages/app-web/src/types/index.ts`
- `packages/app-web/src/services/backendAccess.ts`
- `packages/views/src/local-runtime/LocalRuntimeView.tsx`

## 回滚点

- 若 relay apply 牵动过大，保留领域模型/API，先把 request 停在 `accepted` + 手动 retry，不进入 backend 自动下发。
- 若 Tauri personal 确认面过大，首版 personal 新增任意 root 一律 `rejected`，只支持 managed backend 自动 ack；但数据模型仍保留 personal pending 能力。
- 若 `control_mode` 迁移影响大，先在 `BackendConfig` 加字段并按 migration 一次性填充，不做运行时 fallback 推导。

## 进入实现前检查

- [ ] 用户确认 `design.md` 的五个核心决策。
- [ ] 任务状态切到 `in_progress`。
- [ ] 读取 `trellis-before-dev` 和相关 backend / cross-layer / frontend spec。
- [ ] 先实现领域和 API 测试，再接 relay/local/frontend。

# 本机执行面 Enrollment 路径收束 — 执行计划

> 配套 `prd.md` / `design.md`。三阶段顺序执行；阶段间共享 contract 类型，故 Phase 1 先收口契约再到前端。每阶段有验证命令与回滚点。

## Phase 1 — 后端收束 + 身份去 project 化 + 最小 auth（核心，安全敏感）

### 1.1 统一 enrollment use case
- [ ] 新增 `EnrollmentSource` / `EnrollLocalBackendRequest` / `LocalBackendEnrollment`，实现 `enroll_local_backend`（建议 `agentdash-application/src/backend/enrollment.rs`）。
- [ ] 把 `ensure_local_runtime_record`、`ensure_runner_project_runtime_record`、`ensure_runner_project_runtime_record_with_ports` 合并到唯一路径；删除遗留重复。
- [ ] 统一 token 生成（`generate_backend_auth_token()`）、device 拼装、rotate 语义；两路径都写 `device.registration_source`。

### 1.2 身份去 project 化
- [ ] `stable_local_backend_id` 不再含 project_id；runner backend 落 `share_scope=User(owner=token.created_by_user_id)`、visibility=Shared、profile=`runner-registration`。
- [ ] 确认 `ensure_local_backend` upsert key 与新身份一致。

### 1.3 ProjectBackendAccess 权威 + 最小 auth 规则
- [ ] 保留 claim 时 `ensure_active_project_backend_access`。
- [ ] `authorization.rs` 新增 `user_scoped_grant_allows`：User-scoped backend 有 active grant → 该 project 成员按 permission 放行；无 grant 维持 owner-only。
- [ ] 列表路径批量预取 grant，避免 N+1。

### 1.4 DTO 同构
- [ ] `EnsureLocalRuntimeResponse` 补 `registration_source` + `claimed_at`；两路径共享核心字段。
- [ ] handler 退化为薄适配器（认证 → 构造 source → 调 use case → 映射）。
- [ ] `pnpm run contracts:check` 重生成 TS binding。

### 1.5 后端验证（review gate，安全敏感，必须全绿再进 Phase 2）
```
cargo fmt --all -- --check
cargo test -p agentdash-application
cargo test -p agentdash-api
pnpm run contracts:check
pnpm run migration:guard
```
- [ ] 新增/更新测试覆盖 design §6 后端项（尤其 auth 正反四路 + 同机器跨 project 同 backend_id）。
- **回滚点**：本阶段可独立回退；DTO 与 TS binding 成对回滚。

## Phase 2 — 前端 Runner token UI + 工作空间区心智重构

### 2.0 IA 评估（review gate，先评估再动 UI）
- [ ] 产出工作空间区 IA 评估：现状概念清单 → 用户三问映射 → 目标分组与文案（落 research/ 或 design 增补）。
- [ ] 与用户确认目标 IA 后再进入 2.1/2.2。

### 2.1 Runner token service + UI
- [ ] 补 token service（create/list/revoke/rotate），参照 `services/backendAccess.ts`。
- [ ] Project Settings → 工作空间区加 token 列表/创建/轮换/撤销；create/rotate 一次性明文 + 复制。
- [ ] setup 命令拼装（通用 binary + 显式 origin），origin 取当前云端 origin。
- [ ] token 管理作为「运行环境」子块自然融入（与 2.2 一并设计，避免二次返工）。

### 2.2 工作空间区心智重构（按 2.0 确认的目标 IA 落地）
- [ ] 「Backend Access」→「运行环境 / 可用机器」，grant/priority/inventory 降次级/展开。
- [ ] 本机发现收进 workspace 条目「在某机器上定位」动作 + 「可用机器」内联呈现，去掉独立发现面板来回跳转。
- [ ] Workspace Modules 降级诊断/高级区。
- [ ] 硬约束：不新增回退/断链；`workspaceRouting` / `runtimeDiagnostics` 行为不退化；多 project grant 管理 UI 不在此（归独立任务 06-27-runner-multi-project-access）。

### 2.3 前端验证
```
pnpm --filter app-web run typecheck
pnpm --filter @agentdash/views typecheck
pnpm --filter app-tauri typecheck
pnpm --filter app-web test -- runtimeDiagnostics workspaceRouting
```
- [ ] 新增 token UI 行为测试（明文只显示一次、setup 命令、revoke/rotate 流转）。
- 注：已知 `frontend:lint` 在无关旧文件 `CanvasRuntimeBindingsEditor.tsx` 失败，勿误判为本任务回归。
- **回滚点**：前端独立回退；不影响 Phase 1 后端。

## Phase 3 — 文档 + 手工验收 + spec 更新

- [ ] 文档写清两条用户路径：Desktop（登录→自动 ensure→connect）、Runner（建 token→复制 setup→claim→online）。
- [ ] 手工验收（复用 `06-26-distribution-release-validation` runbook）：
  - [ ] 桌面登录后自动挂 runtime。
  - [ ] 云端建 token→复制 setup→server runner 上线。
  - [ ] revoke/过期 token 后新 claim 被拒，已领 relay credential 生命周期可解释。
  - [ ] 同一 runner 被加入第二个 project 后可见（验证身份去 project 化 + grant 放行；若完整「加入另一 project」UI 在兄弟任务，则用直接写 grant 的方式验证后端模型）。
- [ ] `trellis-update-spec` 把 enrollment 统一模型 + ProjectBackendAccess 权威性沉淀进 `.trellis/spec`。

## 边界外（已拆独立任务）

- **`06-27-runner-multi-project-access`（独立任务，不挂父任务）**：多 project 复用完整授权管理（建立/撤销 grant、priority/policy、跨 owner 策略、审计反向视图）。原 C（引入 Org/Team scope 让 runner 所有权脱离个人）作为该任务的后续追踪项。
- B（Workspace 设置心智重构）已**收回本任务** Phase 2 处理。

## 全局校验（收尾）
```
pnpm install --frozen-lockfile
pnpm run contracts:check
pnpm run migration:guard
pnpm run desktop:check
cargo fmt --all -- --check
```

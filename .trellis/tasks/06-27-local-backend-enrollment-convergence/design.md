# 本机执行面 Enrollment 路径收束 — 技术设计

> 配套 `prd.md`（含 2026-06-27 Decisions）。本文件只写技术设计：边界、契约、数据流、取舍、兼容性与回滚形状。

## 0. 现状事实（review 结论）

- 两个 endpoint 已存在：
  - `crates/agentdash-api/src/routes/backends.rs::ensure_local_runtime`（`CurrentUser`）。
  - `crates/agentdash-api/src/routes/runner_registration_tokens.rs::claim_runner`（公开 + bearer registration token）。
- application 层有**三处近重复** enrollment 逻辑：
  - `agentdash-application/src/backend/management.rs::ensure_local_runtime_record`（desktop）
  - `management.rs::ensure_runner_project_runtime_record`（疑似遗留）
  - `backend/runner_registration.rs::ensure_runner_project_runtime_record_with_ports`（runner 实际走的）
  - 三处各自做：normalize → `stable_local_backend_id` → 拼 `device` → `LocalBackendClaim` → `ensure_local_backend` → 取 auth_token。
- 已知漂移：desktop 用 `generate_backend_auth_token()`，runner 直接 `Uuid::new_v4()`；desktop 尊重 `rotate_token`，runner 硬编码 `false`；desktop 不写 `device.registration_source`，runner 写 `"runner_registration_token"`。
- response 不同构：`EnsureLocalRuntimeResponse` 有 `backend_enabled/profile_id/visibility`、无 `registration_source/claimed_at`；`RunnerRegistrationClaimResponse` 相反。
- scope 模型：`authorization.rs` 鉴权**只读** `share_scope_kind/share_scope_id`，**不读** `ProjectBackendAccess`；后者目前只服务 workspace 绑定同步 + priority。runner backend 同时被 `share_scope=Project` 和一行 `ProjectBackendAccess` 表达「属于 project X」——冗余。
- `stable_local_backend_id = hash(machine_id, share_scope_kind, share_scope_id, capability_slot)`；runner 把 `Project/project_id` 烤进去 → 同机器跨 project 裂成多 backend（1:1 死绑）。
- scope 只有 `User/Project/System`，无 Org/Team。

## 1. 目标与非目标

**目标**：application 层 enrollment 收束为单一 use case；两路径 response 同构；runner backend 身份去 project 化 + `ProjectBackendAccess` 成权威授权层 + 最小 auth 放行；Project 设置内 Runner token 管理 UI + setup 命令；工作空间区轻量心智重构；文档与手工验收。

**非目标**：合并两个 HTTP endpoint；多 project 完整授权管理 UI / priority / policy；Workspace 设置整体 IA 重构；引入 Org/Team scope；runner config / desktop profile 文件格式合并。

## 2. 后端设计

### 2.1 统一 enrollment use case

新增（建议落在 `agentdash-application/src/backend/enrollment.rs`，或就地收束进 `management.rs`）：

```rust
pub enum EnrollmentSource {
    DesktopAccessToken { user_id: String },
    RunnerRegistrationToken { token: RunnerRegistrationToken }, // 已校验过 status
}

pub struct EnrollLocalBackendRequest {
    pub machine_id: String,
    pub machine_label: Option<String>,
    pub capability_slot: Option<String>,
    pub name: Option<String>,
    pub executor_enabled: bool,
    pub client_version: Option<String>,
    pub device: serde_json::Value,
    pub rotate_token: bool,        // desktop 传入；runner 固定 false
    pub relay_ws_url: String,
    pub requested_scope: Option<...>, // desktop 可显式指定，runner 忽略
}

pub struct LocalBackendEnrollment {  // 统一返回核心
    pub backend: BackendConfig,
    pub auth_token: String,
    pub registration_source: RegistrationSource, // enum
    pub claimed_at: DateTime<Utc>,
}

pub async fn enroll_local_backend(
    repos: &RepositorySet,
    source: EnrollmentSource,
    req: EnrollLocalBackendRequest,
) -> Result<LocalBackendEnrollment, ApplicationError>;
```

`enroll_local_backend` 内部按 `source` 决定 scope / owner / profile / visibility / registration_source，再走**唯一一条** normalize → backend_id → device → claim → `ensure_local_backend` → 取 token 路径。三处旧函数删除或改为薄包装后删除。

`device.registration_source` 两条路径都写：desktop = `desktop_access_token`，runner = `runner_registration_token`。token 生成统一用 `generate_backend_auth_token()`。

### 2.2 身份去 project 化

`stable_local_backend_id` 改为不再随 project 变化的机器级身份。两条路径的 backend 落点：

| 维度 | Desktop | Runner（改后） |
|---|---|---|
| share_scope_kind | User | **User**（owner = `token.created_by_user_id`） |
| share_scope_id | user_id | **owner user_id**（不再是 project_id） |
| backend_id hash 输入 | machine + User + user_id + slot | machine + User + owner + slot |
| visibility | Private | Shared |
| profile_id | 请求传入 | `"runner-registration"` |

> 结果：同机器 + 同 slot 的 runner，无论被哪个 project claim，都得到同一稳定 backend_id；project 归属完全由 `ProjectBackendAccess` 行表达。

`ensure_local_backend` 的 upsert lookup key（machine_id, scope_kind, scope_id, slot）随之统一，不再含 project。

### 2.3 ProjectBackendAccess 成权威授权层 + 最小 auth 规则

- claim 流程保留 `ensure_active_project_backend_access(project_id, backend_id, owner)`（已存在），它现在是 project 能看到 runner 的**唯一**依据。
- `authorization.rs` 在现有分支基础上加最小规则：

```
User-scoped backend B：
  - identity == owner → 允许（现状）
  - 否则：若存在 active ProjectBackendAccess(project_p, B) 且 identity 是 project_p 成员且满足 permission → 允许（新增）
```

实现上：在 `project_scope_allows` 同级新增 `user_scoped_grant_allows`，查询 `project_backend_access_repo` 找 B 的 active grant，对每个授权 project 复用现有 project 成员/权限判断。无 grant 时行为与今天完全一致（owner-only），desktop 个人 backend 不受影响 → 不回退。

> 性能：列表场景（`authorization.rs:81-91` 遍历所有 backend）需注意每个 User-scoped backend 多一次 grant 查询。设计上批量预取「当前 identity 可见 project 的 grant 集合」一次，再在内存里判断，避免 N+1。

### 2.4 API DTO 同构

`EnsureLocalRuntimeResponse` 与 `RunnerRegistrationClaimResponse` 共享核心字段（建议提取 `LocalBackendEnrollmentDto` 嵌入或展开）：

共享：`backend_id, name, relay_ws_url, auth_token, machine_id, machine_label, share_scope_kind, share_scope_id, capability_slot, registration_source, claimed_at`。
Desktop 额外保留：`backend_enabled, profile_id`（向用户暴露的语义字段）。

契约改动会触发 TS binding 重新生成（`pnpm run contracts:check`）。因无部署，直接改形状，不做向后兼容包装。

## 3. 前端设计

### 3.1 Runner token 管理 UI（Project Settings → 工作空间区）

- service：补 `packages/app-web/src/services/`（参照 `backendAccess.ts`）对接 create/list/revoke/rotate（contracts 类型已生成）。
- 组件：token 列表（name / status / 过期 / last_used）、create、rotate、revoke；create/rotate 后弹一次性明文 + 「复制 setup 命令」。
- setup 命令模板（通用 binary + 显式 origin）：
  ```
  agentdash-local setup --server <origin> --token <plaintext> \
    --name <runner-name> --workspace-root <path> --install-service --start
  ```
  origin 取自当前云端 origin（窗口 location / 配置）。

### 3.2 工作空间区心智重构（本任务正式评估处理）

**现状术语负担（review 结论）**：工作空间 tab 现为 4 个并列面板（Backend Access / 本机 Workspace 发现 / 工作空间 / Workspace Modules），强迫用户理解 Backend、Access grant、Inventory、`root_ref`、priority、candidate、binding、resolution、discovery、Module 等十余个术语，中英混排、无引导流。

**评估方法（Phase 2 先做、设评估 gate）**：产出一份「现状概念清单 → 用户三问映射 → 目标分组与文案」的 IA 评估（落任务 research 或 design 增补），与用户确认后再改 UI。

**目标 IA（围绕用户三问）**：

| 用户的问题 | 现状对应 | 目标呈现 |
|---|---|---|
| 这个项目能在哪儿跑？ | Backend Access 面板 | 「运行环境 / 可用机器」区：列出可用机器（在线状态），把 grant/priority/inventory 等后台词降到展开/次级；Runner token 管理（接入新服务器）作为本区子块 |
| 我在搞哪份代码？ | 工作空间列表 | 「代码空间」：以代码来源（Git/P4/本地目录）为主语，状态用人话 |
| 代码在那台机器上落在哪？ | 本机发现面板 + binding + resolution | 收进 workspace 条目内的「在某机器上定位」动作 + 「可用机器：A ✓ / B 未定位」内联呈现，去掉独立发现面板的来回跳转 |
| （高级/诊断） | Workspace Modules | 降级到诊断/高级区，不占主线 |

**硬约束**：不引入新的回退/断链；现有 `workspaceRouting` / `runtimeDiagnostics` 行为不退化；改动集中在 IA 分组、术语文案与「发现→绑定」的就地化，复杂的多 project grant 管理 UI 不在此处（归独立任务 `06-27-runner-multi-project-access`）。

> 与 §3.1 的 Runner token UI 是同一区域的两个子块，应一并设计避免二次返工。

## 4. 数据流（收束后）

```
Desktop:  Tauri runtime_start → POST /ensure (access token)
            → enroll_local_backend(DesktopAccessToken{user}, req)
            → User-scoped backend (Private) → LocalBackendEnrollment
Runner:   agentdash-local setup/run → POST /runner/claim (registration token)
            → 校验 token status → enroll_local_backend(RunnerRegistrationToken, req)
            → User-scoped backend (Shared, owner=token creator)
            → ensure_active_project_backend_access(project, backend)
            → record_usage → LocalBackendEnrollment
鉴权:     project 成员列 backend → authorization 读 share_scope(owner)
            + 新增 User-scoped+active grant 放行 → 可见
```

## 5. 兼容性与回滚

- 无部署 → 无数据迁移负担；backend_id 形状改变不影响线上数据。
- 回滚点：每阶段独立可回退（见 implement.md）。后端收束若出问题，可临时恢复旧三函数路径；DTO 改动与 TS binding 同提交，回滚需成对回退。
- 风险最高项 = 2.3 的 auth 放行规则（安全敏感）。必须正反路径测试齐全后才进入前端阶段。

## 6. 测试策略

- 后端（`cargo test -p agentdash-application` / `-p agentdash-api`）：
  - 同机器跨 project claim → 同一 backend_id（身份去 project 化）。
  - registration_source 两路径稳定写入。
  - auth：owner 可见 / 非 owner 无 grant 不可见 / 非 owner 有 active grant 可见 / grant revoke 后不可见。
  - rotate / token 生成统一。
- 前端（`pnpm --filter app-web test`）：token UI 行为、明文只显示一次、setup 命令拼装、`runtimeDiagnostics` 不回退。
- 手工：PRD 三条手工验收 + token revoke/rotate 后 claim 行为。

# BYOK 与设置页信息架构重构实施计划

## Entry Gate

- 进入实现前运行 `trellis-before-dev`，读取 backend、cross-layer、frontend、shared 相关 spec。
- 实现开始前确认当前任务仍为 `.trellis/tasks/05-28-byok-settings-information-architecture`，并在通过规划 review 后执行 `task.py start`。

## Step 1. Contract 与领域模型

- 在 `crates/agentdash-contracts` 新增 `llm_provider.rs`，定义 admin DTO、effective DTO、user credential DTO、probe request/response。
- 更新 `generate_ts.rs` 和前端 generated 输出，运行 `pnpm run contracts:generate`。
- 在 `agentdash-domain::llm_provider` 中新增 `LlmCredentialMode`、`LlmCredentialSource`、`LlmProviderUserCredential`、credential repository trait。
- 调整 `LlmProvider`，移除运行态明文 `api_key` 字段，增加 `credential_mode` 与 encrypted global key 字段。

## Step 2. 数据库与基础设施

- 新增 PostgreSQL migration `0065_llm_provider_byok.sql`。
- 更新 `PostgresLlmProviderRepository` 的 columns、row mapper、create/update/list。
- 新增 `PostgresLlmProviderCredentialRepository`，实现 user credential CRUD。
- 在 repository bootstrap 和 `RepositorySet` 装配新 credential repo。
- 新增 `SecretCipher` / credential encryption helper，接入 global/user key 写入与读取。
- 增加 migration/repository integration 测试，覆盖 credential mode、user credential unique key、delete cascade。

## Step 3. API 权限与 DTO 映射

- 将 `routes/llm_providers.rs` 的 request/response 切换到 contract DTO 或 route-local 到 contract 的显式 mapper。
- Admin `/llm-providers` create/update/list 支持 `credential_mode`、global key configured/preview/source 字段。
- 新增用户视角：
  - `GET /llm-providers/effective`
  - `PUT /llm-providers/{id}/user-credential`
  - `DELETE /llm-providers/{id}/user-credential`
  - `POST /llm-providers/{id}/probe-models`
- 调整 probe 逻辑：管理员编辑可使用全局候选，普通用户只使用临时 key 或自己的 BYOK。
- 补 API tests：admin vs non-admin，全局写保护，用户 credential 隔离，masked response。

## Step 4. Discovery 与 PiAgent 运行态解析

- 修改 `AgentConnector::discover_options_stream` 签名，新增 `DiscoveryContext { working_dir, identity }`。
- 更新所有 connector 实现、Composite 转发、测试 fake connector。
- `routes/discovered_options.rs` 增加 `CurrentUser` extractor，传入 discovery identity。
- PiAgent provider registry 新增 effective provider 构建：
  - discovery 按当前用户身份解析可执行 Provider。
  - prompt 按 `ExecutionContext.session.identity` 解析可执行 Provider。
  - 无身份系统执行只使用全局 credential source。
- 覆盖 resolver matrix 测试和 PiAgent connector discovery/prompt 测试。

## Step 5. 前端 API、Store 与 View Model

- 前端 `api/llmProviders.ts` 改为消费 generated DTO；新增 mapper 做基础运行时验证。
- 拆分 store：
  - `llmProviderAdminStore`：管理员 catalog CRUD。
  - `llmByokStore`：当前用户 effective list、user credential save/delete/probe。
- user scope settings 增加个人默认模型偏好读写 helper，复用现有 `/settings?scope=user`。
- 更新 `useExecutorDiscoveredOptions` 在 BYOK 保存/删除后刷新 discovery key。

## Step 6. 设置页拆分

- 将 `SettingsPage.tsx` 收敛为 shell。
- 新建 `features/settings` 的 shell/nav 组件。
- 提取现有 Backend 管理为 `features/backend-settings/ui/BackendManagementPanel.tsx`。
- 提取全局 Provider 管理为 `features/llm-providers/ui/AdminProviderPanel.tsx`。
- 提取用户 BYOK 面板为 `features/llm-providers/ui/UserByokPanel.tsx`。
- 提取 `ModelManagementSection`、model edit rows、credential form，清理 inline SVG 和重复 local UI atom。
- 提取 Pi Agent / Default Executor 到 `features/agent-settings`。
- 保留 `LocalRuntimeView` 挂载方式，只移动到 shell panel。

## Step 7. UX 状态与错误收口

- 管理员视角显示 Provider credential mode、global key 状态、user override 策略。
- 用户视角显示 effective status：平台提供、个人 Key 生效、需要 BYOK、不可用。
- Provider 无可用凭据时，ExecutorSelector 不展示可执行模型；用户 BYOK 页面仍展示可配置入口。
- prompt/runtime 错误消息指向个人 BYOK 设置，但不包含 secret 内容。
- 移动/窄宽度检查表单、chips、按钮文本不溢出。

## Validation Commands

按风险从窄到宽执行：

```powershell
pnpm run contracts:check
cargo test -p agentdash-domain llm_provider
cargo test -p agentdash-infrastructure llm_provider
cargo test -p agentdash-api llm_provider
cargo test -p agentdash-executor pi_agent
pnpm run frontend:check
pnpm run frontend:lint
pnpm run frontend:test -- Settings
```

最终合并前执行：

```powershell
pnpm run backend:check
pnpm run frontend:check
pnpm run contracts:check
```

如果 UI 改动较大，启动：

```powershell
pnpm dev
```

并用浏览器检查 `/settings` 在管理员、普通用户、desktop runtime 可用/不可用状态下的布局。

## Risk Points

- `AgentConnector::discover_options_stream` 签名改动会触达所有 connector 和测试 fake，需要一次性编译收口。
- `llm_providers.api_key` 退出运行态会影响旧开发数据；迁移后需要通过 UI 重新保存 DB-backed global key 或配置 `env_api_key`。
- `openai_codex` 现有 OAuth credential 是 JSON 字符串，进入加密字段时要保持原 JSON 字符串原样加密/解密，不能对结构做二次变形。
- Settings UI 拆分过程中要保持 Codex OAuth poll/cancel 流程、probe models 流程和 discovery refresh 行为不丢失。
- BYOK resolver 必须是 prompt、discovery、probe 的唯一事实源，避免三个路径各自拼解析规则。

## Rollback Points

- 完成 Step 2 后先跑 repository tests；如果 migration 或 encryption helper 不稳定，先停在后端 schema 层修正，不进入 connector 签名改动。
- 完成 Step 4 后先跑 executor/API 窄测试；如果 discovery 身份传递出现跨 connector 回归，先修 trait 调用链，不进入前端拆分。
- 完成 Step 6 后用 `git diff --stat` 检查 `SettingsPage.tsx` 是否确实收敛，避免只搬动少量代码但留下双份逻辑。

## Definition of Ready for Implementation

- `prd.md`、`design.md`、`implement.md` 已写入任务目录。
- 用户确认采用“管理员维护 Provider Catalog，用户只维护个人 BYOK 凭据与默认模型偏好”的产品边界。
- 任务状态从 `planning` 切到 `in_progress` 后再开始代码实现。

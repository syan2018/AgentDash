# 推进记录

## 2026-05-28

已完成 BYOK 主链路实现与质量检查。

### 已实现

- LLM Provider Catalog 增加 `credential_mode` 与全局密文 Key 字段，领域层移除明文 `api_key`。
- 新增用户 BYOK 凭据实体、仓储接口、PostgreSQL migration 与仓储实现。
- 新增 `LlmSecretCodec` 端口与基础设施 AES-GCM 实现，DB-backed global/user Key 只以密文保存。
- 修正 master key 来源：`AGENTDASH_SECRET_KEY` 仅作为显式覆盖，未配置时自动在 AgentDash 数据根创建并复用本地 master key 文件，避免本地设置页无法保存/探测全局 Key。
- API 增加 generated contract DTO、管理员 Provider 管理字段、当前用户 effective provider list、用户 credential save/delete、用户 scoped probe。
- Discovery 与 PiAgent prompt 都通过当前身份解析 effective Provider，`global_only` / `global_or_user` / `user_required` 共用 resolver。
- 前端 API / store 切换到 generated LLM Provider contracts，新增用户 BYOK store 和个人 BYOK 面板。
- 设置页入口 `pages/SettingsPage.tsx` 收敛为 feature re-export，通用 setting primitives、DebugPrefs、User BYOK 面板已拆出到 `features/settings/ui`。
- 更新 LLM model config 与 frontend/backend contract specs，记录 Provider Catalog、BYOK 凭据与 generated contract 契约。

### 验证

- `cargo test -p agentdash-domain llm_provider` 通过，覆盖 5 个 resolver 矩阵用例。
- `cargo test -p agentdash-executor pi_agent` 通过，50 个用例。
- `cargo test -p agentdash-infrastructure secret_cipher_roundtrips_plaintext` 通过。
- `cargo test -p agentdash-infrastructure secret` 通过。
- `pnpm run frontend:check` 通过。
- `pnpm run frontend:lint` 通过。
- `pnpm run contracts:check` 通过。
- `pnpm run backend:check` 通过。
- `cargo check -p agentdash-api` 通过。

### 剩余建议

- 设置页业务主体仍可继续按面板深化拆分，尤其是全局 Provider 管理、模型 chip/editor、Backend 管理与 Agent/Executor 设置。
- 若进入提交前收尾，可补 API route 级权限测试与用户默认 Provider/Model 偏好 UI 的更完整回归。

### 第二笔迭代：Codex 用户 OAuth

- 新增用户侧 Codex OAuth 入口，允许 `global_or_user` / `user_required` 的 `openai_codex` Provider 把 ChatGPT OAuth token JSON 保存为当前用户 BYOK 凭据。
- 管理员全局 Codex 登录和用户个人 Codex 登录复用 `OAuthLoginWizard`，登录向导统一负责启动 flow、打开外部浏览器、轮询状态、取消和完成刷新。
- 用户 BYOK 面板对 `openai_codex` 不再展示 API Key 输入框；保存 API Key 的通用接口也会拒绝 Codex Provider，避免生成不可执行的个人凭据。
- Codex 凭据 preview 收敛为 OAuth 状态文案，不再对 token JSON 做掩码展示。
- Codex OAuth 已配置状态在管理员和用户面板中都有独立验证标识，重新授权动作使用“重新验证 ChatGPT”表达。
- 手动 BYOK Key 保存会同步探测模型接口并持久化验证状态；已保存 Key 可通过“验证”按钮重新检查，用户面板展示已验证 / 验证失败 / 未验证。
- 后续用户自定义 LLM Provider 采用用户私有 Provider overlay 方向，和管理员全局 Catalog 分层，避免个人 endpoint 进入团队共享 Provider 事实源。
- 验证通过：`cargo check -p agentdash-api`、`cargo test -p agentdash-api codex`、`pnpm run contracts:check`、`pnpm run frontend:check`、`pnpm run frontend:lint`、`pnpm run backend:check`。
- 已重启 `pnpm dev` 并用浏览器验证：临时 `global_or_user` Codex Provider 在个人 BYOK 面板展示 ChatGPT 登录入口，用户 OAuth start/cancel 接口可用，临时 Provider 已清理。

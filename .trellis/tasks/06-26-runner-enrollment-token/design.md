# Runner 注册令牌与云端领取流程 - Design

## Architecture

新增 runner registration token 作为服务器托管 runner 的授权入口。桌面端继续使用用户 access token 领取 personal runtime；独立 Local Runner 使用 registration token 领取 project-scoped runner。

第一版选择项目范围 token：token 在某个 Project 下创建，领取出的 backend 自动获得该 Project 的 backend access。这样服务器 runner 的权限边界与项目托管场景一致。

## API Shape

新增项目级 API：

- `POST /api/projects/{project_id}/runner-registration-tokens`：创建 token，返回明文 token 一次。
- `GET /api/projects/{project_id}/runner-registration-tokens`：列出 token 元数据，不返回明文。
- `POST /api/projects/{project_id}/runner-registration-tokens/{token_id}/revoke`：撤销 token。
- `POST /api/projects/{project_id}/runner-registration-tokens/{token_id}/rotate`：撤销旧 token 并返回新明文 token。
- `POST /api/local-runtime/runner/claim`：runner 使用 registration token 领取 `backend_id`、`relay_ws_url`、`auth_token`。

桌面端现有 `/api/local-runtime/ensure` 继续用于 access-token based desktop runtime，不混用 runner token。

## Persistence

Registration token 只存哈希，不存明文。记录字段至少包含：

- token id、project id、name、token hash、created_by。
- expires_at、revoked_at、last_used_at。
- capability_slot、machine policy、created_at、updated_at。

Token 创建时明文只在响应中出现一次。撤销后不能再次领取。过期策略第一版默认需要显式过期时间，UI 可提供推荐值。

## Claim Flow

Runner claim 请求包含 registration token、machine id、machine label、runner name、client version、device payload、executor_enabled。云端校验 token 后：

- 创建或更新 local backend 记录。
- 建立 ProjectBackendAccess。
- 生成 runner 专用 `auth_token` 与 `relay_ws_url`。
- 更新 token `last_used_at`。

## Tradeoffs

- 使用独立 claim endpoint，让桌面 access token 路径和 runner registration token 路径在授权、审计和错误提示上清晰分离。
- 项目范围 token 比用户范围 token 更适合服务器 runner，避免 runner 被用户个人会话语义牵制。

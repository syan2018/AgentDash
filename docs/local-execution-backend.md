# 本机执行面 / Local Execution Backend

AgentDash 的「本机执行面」是指真正在某台机器上承载 agent 执行的后端。它有两种接入形态，共享同一套云端 enrollment 语义、relay 凭据形状和诊断事实，只是**授权入口不同**：

| 形态 | 谁的机器 | 认证入口 | 默认归属 |
|---|---|---|---|
| **Desktop Local Runtime** | 用户自己的这台设备 | 已登录桌面 App 的用户 access token | user / 个人 |
| **Standalone Local Runner** | 无 UI 的服务器 | 项目级 runner registration token | 通过 `ProjectBackendAccess` 授权给 project |

两者最终都从云端拿到同一组 relay 凭据 `{ backend_id, relay_ws_url, auth_token }`，并以 `auth_token` 连接 `/ws/backend`。`registration_source` 字段稳定区分二者（`desktop_access_token` vs `runner_registration_token`），诊断 UI 与「运行环境」列表据此打标。

---

## 路径 A：Desktop Local Runtime（你自己的电脑）

1. 安装并打开桌面 App，登录。
2. App 用你的 access token 自动调用 `POST /api/local-runtime/ensure`。
3. 云端创建/复用一个 **user-scoped** 本机 backend，返回 `{ backend_id, relay_ws_url, auth_token }`。
4. 本机 runtime 用 `auth_token` 连接 relay，上线。

你**不需要**理解或复制任何 token；设置页「运行环境」会把这台机器标为「本机（这台设备）」。

---

## 路径 B：Standalone Local Runner（服务器）

1. 在 **Project 设置 → 工作空间 → 运行环境 → 接入新服务器** 创建一个 runner registration token。
   - token 明文（`adrt_<id>_<secret>`）**只展示一次**，请立即复制。
2. 复制生成的安装命令，到服务器上执行：
   ```bash
   agentdash-local setup --server <origin> --token adrt_... \
     --name <runner-name> --workspace-root <path> --install-service --start
   ```
3. `setup` 用 token 调用 `POST /api/local-runtime/runner/claim`，云端：
   - 创建/复用一个 **机器级**（不绑定具体 project 的）本机 backend；
   - 写入一行 active `ProjectBackendAccess(project, backend)`，把这台 runner 授权给当前 project；
   - 返回 `{ backend_id, relay_ws_url, auth_token }`，并把凭据写回本机 config。
4. service 自动启动，runner 用 `auth_token` 连接 relay、上线。

要点：
- registration token 只用于 claim，**绝不**用于 `/ws/backend`（relay 只认 `auth_token`）。
- token 可在同一界面 **撤销 / 轮换**；撤销/过期后新的 claim 会被拒绝。
- 一台 runner 的身份是机器级的：同一台机器（同 capability slot）无论被哪个 project 接入，都是同一个 `backend_id`，project 归属由 `ProjectBackendAccess` 表达。
  > 注：从 project 设置里「把一台已有 runner 授权给另一个 project」的完整管理界面属于后续任务（`06-27-runner-multi-project-access`）。

---

## 不要这样做

- 不要让 server runner 保存用户 access token —— 服务器常驻进程应使用可撤销/可轮换/可审计的 project registration token。
- 不要把 registration token 当成 relay token 发给 `/ws/backend`。
- 不要把日志 / status / doctor / UI 复制内容里的 access token、registration token、relay auth token 输出明文 —— 这些字段必须脱敏。

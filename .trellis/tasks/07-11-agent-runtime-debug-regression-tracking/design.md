# Agent Runtime 调试回归修复设计

## 1. Execution Profile 控制面

用户选择的是产品级 execution profile，而不是已经实例化的 Runtime offer。控制面从 Integration definition 与平台产品契约投影稳定 profile identity、显示名称、能力边界、availability 和 unavailable reason；首次 AgentRun provision 再根据 profile、Provider、模型、credential scope 与 placement 创建 instance/offer/binding。

API 恢复前端当前需要的 discovery 与 discovered-options 能力，但实现的数据源必须是新的 Runtime definition registry、LLM Provider catalog 与业务 execution profile projection，不恢复 Connector discovery。前端展示不可用项并说明原因，不通过过滤制造空列表。

## 2. 全局 Provider 与 OAuth 授权

全局 Provider credential 属于平台配置，不属于发起 OAuth 的用户。短时 OAuth flow 仍绑定发起 identity 以防止 flow 劫持，但 identity 由 API 的 AuthProvider 解析：Personal 模式允许无 Bearer header 并得到 local identity；Enterprise 模式必须由真实 access token 和 admin 权限通过服务端裁决。

Desktop bridge 的 token 因此是可选传输字段。Web 与 Tauri 不提前判断登录状态；存在 token 时附加 Bearer，不存在时不发送 Authorization。API 保留 `CurrentUser`、`require_system_access` 与 flow owner 校验，最终根据 target 将凭据保存到 global provider 或 user BYOK 的正确存储位置。

## 3. 验证边界

- API/前端测试覆盖 execution profile 可见性、不可用原因与 Provider/model options。
- Desktop OAuth 测试覆盖无 token 不附 Authorization、有 token 正确附加 Bearer。
- API auth 测试覆盖 Personal 无 token、Enterprise 401、Enterprise non-admin 403 与 admin 成功。
- `pnpm dev` 验证 ProjectAgent 配置与首次 AgentRun，以及 openai_codex 全局 Provider 的桌面 OAuth 启动链。

# Relay WebSocket 鉴权与注册收口

## Goal

补齐本机后端接入 cloud relay 的鉴权和注册约束，避免当前“能连上但未严格校验”的灰区状态。

## Background

当前 `03-18-cloud-local-boundary-refactor` 已经把运行时边界收紧到：

- 第三方 Agent 必须通过 relay 路由到 `Workspace.backend_id`
- workspace 文件接口只认正式 backend 归属

但 relay 连接本身仍然存在安全收口不完整的问题：

- `ws_backend_handler` 会读取 `token`，但尚未完成真正的 token 校验
- register 消息中的 `backend_id` 与服务端登记配置尚未形成强绑定
- 鉴权失败、backend_id 不匹配、重复注册等场景还缺少明确错误语义

## Requirements

- 在 cloud 侧完成 WebSocket `token` 校验
- 建立 `token -> backend_id` 的服务端权威绑定
- register 时校验 `payload.backend_id` 与 token 绑定值一致
- 对非法 token、backend_id 不匹配、禁用 backend、重复注册冲突等场景返回明确错误
- 保留当前 relay 生命周期与在线状态模型，不引入额外复杂基础设施

## Acceptance Criteria

- [ ] 未携带 token 或 token 无效时，连接无法进入注册成功状态
- [ ] token 对应 backend 与 register payload 中的 backend_id 不一致时，连接被拒绝
- [ ] 被禁用 backend 不能成功注册为在线
- [ ] 鉴权/注册失败路径有明确日志字段和错误响应语义
- [ ] `docs/relay-protocol.md` 与实现保持一致

## Technical Notes

- 预研阶段可继续沿用简单 token 存储，不需要引入复杂密钥服务
- 建议优先沿用现有 `backend_repo` / `BackendConfig.auth_token` 建立绑定
- 该任务只处理 relay 接入安全，不负责 workspace 文件能力和上下文解析能力

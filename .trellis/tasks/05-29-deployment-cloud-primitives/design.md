# 云端发布原语与发现端点设计

## Boundaries

本子任务负责云端部署契约。主要触达范围：

- `crates/agentdash-api`
- `crates/agentdash-infrastructure`
- 可能涉及 `scripts/desktop-build.js` 或前端类型时，只更新共享契约说明，不实现桌面端配置体验。
- `docs/deployment/deployment-baseline.md`

不负责：

- `deploy/compose/*` 的完整 Compose 实现。
- Tauri 服务器配置 UI。
- 自动更新。

## Endpoint Contracts

`GET /api/version` 用于部署维护和 health 后版本核对。

建议字段：

```json
{
  "version": "0.2.3",
  "git_sha": "abc1234",
  "build_time": "2026-05-29T12:00:00Z",
  "schema_version": 67
}
```

`GET /.well-known/agentdash` 用于桌面端发现和兼容检查。

建议字段：

```json
{
  "product": "AgentDash",
  "public_origin": "https://agentdash.example.internal",
  "api_base_url": "https://agentdash.example.internal/api",
  "relay_ws_url": "wss://agentdash.example.internal/relay/ws",
  "server_version": "0.2.3",
  "min_desktop_version": "0.2.0",
  "recommended_desktop_version": "0.2.3",
  "relay_protocol_version": "3"
}
```

## Configuration

部署配置以 `AGENTDASH_PUBLIC_ORIGIN` 为中心。缺失 public origin 时，部署 profile 应报错；开发 profile 可以从 host/port 推导 localhost origin。

`DATABASE_URL` 在部署 profile 中必填。开发 profile 可以继续使用现有 PostgreSQL runtime。

## CLI Shape

目标形态：

```bash
agentdash-server serve
agentdash-server migrate
agentdash-server doctor
```

第一轮可以按风险拆分：

- 先实现 endpoint 与配置契约。
- 再拆 migration / doctor 子命令。

如果保留当前启动时 migration，需要在文档中说明它是 readiness 保护，不替代部署流程里的显式 migration。

# 桌面端服务器指向与发布策略设计

## Boundaries

主要触达范围：

- `packages/app-tauri`
- `crates/agentdash-local-tauri`
- `scripts/desktop-build.js`
- `docs/desktop-dev.md`
- `docs/deployment/deployment-baseline.md`

不负责：

- 云端 endpoint 实现。
- Compose / Dockerfile。
- 自动更新完整系统。

## Target Flow

```text
load saved server origin
fallback to build-time default origin
fallback to manual input
GET /.well-known/agentdash
validate product marker
validate desktop version compatibility
save server origin
continue login / pairing / local runtime management
```

## Release Package Types

通用包：

```text
没有默认服务器地址，首次启动输入 server URL。
```

预配置包：

```text
构建时写入默认 server URL，但运行时仍可修改。
```

## API Mode Semantics

- `builtin`：个人本机体验、开发调试、演示包。
- `external`：默认连接外部服务器，是部署服务器场景的主发布模式。
- `sidecar`：本机携带外部 API binary 的特殊形态，后续再决策。

## Compatibility

桌面端连接前读取 discovery：

- `min_desktop_version`。
- `recommended_desktop_version`。
- `relay_protocol_version`。

版本不满足最低要求时阻止连接；低于推荐版本时允许连接但提示升级。

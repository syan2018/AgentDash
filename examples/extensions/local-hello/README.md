# Local Hello Extension

`local-hello` 是一个独立的 AgentDash extension 示例，用 packaged archive 验证 SDK、panel webview、RuntimeGateway 和本机 TS extension host 的闭环。

完整插件系统开发模型见 [插件系统文档](../../../docs/extension-system.md)。需要 protocol channel、用户自写协议 adapter 和 process/workspace 示例时，看 `examples/extensions/protocol-demo`。

## 开发命令

```powershell
pnpm --dir examples/extensions/local-hello run dev
pnpm --dir examples/extensions/local-hello run validate
pnpm --dir examples/extensions/local-hello run pack
pnpm --dir examples/extensions/local-hello run test
```

安装到本地 AgentDash Project 时，传入平台 API、Project ID 和 token：

```powershell
pnpm --dir examples/extensions/local-hello run agentdash:install -- --api-url http://127.0.0.1:3001 --project <project-id> --token <token> --overwrite
```

安装辅助命令使用 `agentdash:install` 名称，因为 packaged extension artifact 会保留 `package.json`，平台校验会把 npm lifecycle scripts 视为安装期执行入口。

## 行为

- `local-hello.profile` action 通过 `ctx.api.local.getProfile()` 读取本机 runtime profile。
- `local-hello.panel` workspace tab 加载 `dist/panel/index.html`。
- panel 通过 `@agentdash/extension-ui` bridge 调用 action，并展示 username、platform、backend 和 session 摘要。

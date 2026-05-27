# Protocol Demo Extension

`protocol-demo` 展示完整 TypeScript extension host 心智：普通 TS action、用户自写协议 adapter、built-in Host API facade、protocol channel provider，以及通过 `ctx.api.channels.self()` 调用自有 channel。

更多开发模型见 [插件系统文档](../../../docs/extension-system.md)。

## 开发命令

```powershell
pnpm --dir examples/extensions/protocol-demo run dev
pnpm --dir examples/extensions/protocol-demo run validate
pnpm --dir examples/extensions/protocol-demo run test
pnpm --dir examples/extensions/protocol-demo run pack
```

`dev` 会启动本地 Extension Preview，panel 通过现有 `@agentdash/extension-ui` bridge 调用本地 extension host dispatcher。`Protocol Demo` 可在不 pack、不安装 Project 的情况下验证纯 TS action、provider channel、自有 channel 与 dependency alias 调用。

安装到本地 AgentDash Project：

```powershell
pnpm --dir examples/extensions/protocol-demo run agentdash:install -- --api-url http://127.0.0.1:3001 --project <project-id> --token <token> --overwrite
```

## 行为

- `protocol-demo.greet` 是纯 TypeScript action。
- `protocol-demo.workspace_demo` 通过 `ctx.api.workspace` 写入、读取、stat 和 list workspace 文件。
- `protocol-demo.shell_demo` 通过 `ctx.api.process.shell()` 执行通用 shell，并通过 `ctx.api.env.get("PATH")` 读取本机环境事实。
- `protocol-demo.api` 是 provider channel；`protocol-demo.consume_demo_channel` 使用 self shortcut 调用同一个 extension host 维护的 channel method。
- `protocol-demo.panel` 通过 `@agentdash/extension-ui` bridge 调用 action，并展示返回的 JSON 摘要。

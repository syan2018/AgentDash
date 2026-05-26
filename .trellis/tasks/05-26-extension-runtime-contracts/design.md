# Design

## Contract Shape

`ExtensionTemplatePayload` 增加四类声明：

```jsonc
{
  "runtime_actions": [{
    "action_key": "local-hello.profile",
    "kind": "session_runtime",
    "description": "Read local profile",
    "input_schema": {},
    "output_schema": {},
    "permissions": ["local.profile.read"]
  }],
  "workspace_tabs": [{
    "type_id": "local-hello.profile-panel",
    "label": "Local Hello",
    "uri_scheme": "local-hello",
    "renderer": { "kind": "webview", "entry": "dist/panel/index.html" }
  }],
  "permissions": [{ "kind": "local_profile", "access": "read" }],
  "bundles": [{
    "kind": "extension_host",
    "entry": "dist/extension.js",
    "digest": "sha256:..."
  }]
}
```

## Projection

Extension runtime 作为 Project 级全局 runtime surface，而不是 Session 私有字段。独立 `extension_runtime` application/API/contract/frontend service 模块从 enabled Project extension installations 读取 manifest，生成：

- installations
- commands
- flags
- message_renderers
- runtime_actions
- workspace_tabs
- permissions
- bundles

Projection 只表达声明和可见性，不执行 action，也不解析 artifact 内容。Session construction 可读取同一份 Project projection 来获得启动/检查时上下文，但不把 `extension_runtime` 作为 `/sessions/{id}/context` 的主要事实源。

## Module Boundary

- `agentdash-application::extension_runtime`：Project enabled installations -> typed runtime projection 与冲突检测。
- `agentdash-api::routes::extension_runtime`：Project 级 HTTP API。
- `agentdash-contracts::extension_runtime`：独立生成 `extension-runtime-contracts.ts`。
- `packages/app-web/src/services/extensionRuntime.ts`：前端 mapper 与 fetcher。
- Shared Library 只负责 extension template 安装来源，不承载 runtime projection。

## Conflict Policy

同一 Project runtime surface 中：

- `runtime_actions[].action_key` 唯一。
- `workspace_tabs[].type_id` 唯一。
- `workspace_tabs[].uri_scheme` 唯一。

冲突属于配置错误，session construction 应返回可诊断错误，而不是隐式覆盖。

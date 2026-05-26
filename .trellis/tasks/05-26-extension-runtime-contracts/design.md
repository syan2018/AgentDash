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

Session construction 从 enabled Project extension installations 读取 manifest，生成：

- installations
- commands
- flags
- message_renderers
- runtime_actions
- workspace_tabs
- permissions
- bundles

Projection 只表达声明和可见性，不执行 action，也不解析 artifact 内容。

## Conflict Policy

同一 Project runtime surface 中：

- `runtime_actions[].action_key` 唯一。
- `workspace_tabs[].type_id` 唯一。
- `workspace_tabs[].uri_scheme` 唯一。

冲突属于配置错误，session construction 应返回可诊断错误，而不是隐式覆盖。

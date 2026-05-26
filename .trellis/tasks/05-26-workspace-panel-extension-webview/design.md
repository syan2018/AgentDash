# Design

## Dynamic Registry

Built-in tabs 继续由 `registerBuiltinTabTypes()` 注册。Extension tabs 由 runtime projection 派生：

```text
extension_runtime.workspace_tabs -> TabTypeDescriptor[]
```

Registry 需要支持 contribution lifecycle：session 切换或 extension runtime 更新时替换 extension-owned descriptors。

## Webview Host

Plugin tab renderer 加载 sandboxed iframe：

- source comes from packaged panel bundle URL
- sandbox disallows top-level access
- bridge uses postMessage with strict origin/session/tab checks

## Bridge Commands

- `runtime.invokeAction`
- `workspace.openTab`
- `vfs.read/write`
- `events.subscribe/emit`
- `metadata.getContext`

Parent page turns bridge commands into authenticated platform calls.

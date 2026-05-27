# Design

## Dynamic Registry

Built-in tabs 继续由 `registerBuiltinTabTypes()` 注册。Extension tabs 由 Project scoped runtime projection 派生：

```text
extension_runtime.workspace_tabs -> TabTypeDescriptor[]
```

Registry 需要支持 contribution lifecycle：project 切换或 extension runtime 更新时替换 extension-owned descriptors。Extension runtime 前端状态使用独立模块承载，WorkspacePanel 只消费该模块产出的 tab descriptors 与 webview bridge，不把插件运行时状态塞进 session store 或 shared library。

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

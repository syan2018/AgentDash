# Canvas Host SDK

Canvas iframe 通过版本化 MessageChannel 获得 `window.agentdash`。不要使用 `window.parent.postMessage`
发送业务请求，也不要在 source 中保存 credential、Project/AgentRun ID、authority revision、
backend identity 或本机路径。

## SDK

```ts
// SourceBundle manifest 固定的 host action
window.agentdash.actions.invoke(
  actionKey,
  payload,
  { expectedStateRevision? },
)

// renderer runtime surface 已投影的 exact Operation
window.agentdash.operations.list()
window.agentdash.operations.describe(operationRef)
window.agentdash.operations.invoke(operationRef, input, { idempotencyKey? })

window.agentdash.assets.url(vfsUri)
window.agentdash.assets.revoke(objectUrl)

window.agentdash.interaction.getState()
window.agentdash.interaction.dispatch(operationRef, payload, expectedRevision)
window.agentdash.interaction.emit(operationRef, payload, expectedRevision)

window.agentdash.agent.submit({ text?, input?, client_command_id? })
window.agentdash.diagnostics.report(observation)
```

## 路由

| 需求 | 接口 | 参考 |
| --- | --- | --- |
| 触发 Canvas 自定义按钮、刷新或表单提交对应的 host action | `actions.invoke` | `interaction-runtime.md` |
| renderer 直接调用当前 runtime surface 已投影的 exact Operation | `operations.*` | `runtime-actions.md` |
| 显示当前 resource surface 的图片 | `assets.*` | `vfs-assets.md` |
| 读取或修改 canonical UI state | `interaction.*` | `interaction-state.md` |
| 用户把反馈交给当前 Agent | `agent.submit` | `agent-submit.md` |
| Agent 创建、编辑、检查和展示 Canvas | Workspace Module/VFS 工具 | `agent-side-interfaces.md` |

`actions.invoke` 只提交 action key 与 payload；可信 host 从当前 revision 的
`canvas.json.actions` 解析 SourceBundle 固定的 OperationScript、Operation 或 platform command。
这个 action 不需要出现在 `operations.list()` 中。

`operations.*` 只面向 renderer 当前 runtime snapshot 中的 Operation catalog。即使 Canvas attach
了 AgentRun，`operations.list()` 也不代表该 Agent 的完整工具面；只有明确投影到这个 Canvas /
Interaction runtime surface 的 VFS、MCP、Extension 或 Interaction Operation 才会出现并可调用。

host 对请求数、payload 大小、超时、frame identity 与 generation 做限制。iframe reload/unmount
会取消未完成请求并释放由 host 创建的 asset URL。

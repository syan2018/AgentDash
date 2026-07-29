# Canvas Host SDK

Canvas iframe 通过版本化 MessageChannel 获得 `window.agentdash`。不要使用 `window.parent.postMessage`
发送业务请求，也不要在 source 中保存 credential、Project/AgentRun ID、authority revision、
backend identity 或本机路径。

## SDK

```ts
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
| 调用平台、MCP、Extension 或 Interaction 能力 | `operations.*` | `runtime-actions.md` |
| 显示当前 resource surface 的图片 | `assets.*` | `vfs-assets.md` |
| 读取或修改 canonical UI state | `interaction.*` | `interaction-state.md` |
| 用户把反馈交给当前 Agent | `agent.submit` | `agent-submit.md` |
| Agent 创建、编辑、检查和展示 Canvas | Workspace Module/VFS 工具 | `agent-side-interfaces.md` |

host 对请求数、payload 大小、超时、frame identity 与 generation 做限制。iframe reload/unmount
会取消未完成请求并释放由 host 创建的 asset URL。

# Workspace Module / Canvas 双工交互现状审计

本文件综合了 OperationProgram、Canvas/Extension UI 两路独立只读审计和最终架构一致性复核；同步/异步、RuntimeGateway、component authority 与验证切片已按交叉审稿收紧。

## 1. 审计结论

用户观察到的产品机会成立，但现状需要准确表述：Canvas 能借助浏览器 JavaScript 串联 MCP、Runtime Action 与部分 Extension protocol 调用，却没有独立、可序列化、可预检、可取消的解释执行器。

当前也不是通用双工交互系统，而是三条局部链路：

1. Canvas 是 Project scoped 的持久代码资产，Agent 通过 VFS 直接编辑它。
2. 浏览器 iframe 维护本地交互状态，后端只保存 AgentRun scoped latest snapshot 供 Agent 观察。
3. Extension 主要贡献完整 Workspace Tab，没有可被 Canvas 组合的 component ABI。

正确的演进不是增加更多 Canvas/Session bridge，而是抽出 `OperationProgram` 与 `InteractionInstance` 两个一等对象。

## 2. “解释执行”实际链路

### Canvas 保存的是代码，不是 IR

- `crates/agentdash-domain/src/canvas/entity.rs` 保存 `entry_file`、sandbox/import map 与 source files。
- `crates/agentdash-domain/src/canvas/value_objects.rs` 的文件模型是 `{ path, content }`。
- PostgreSQL repository 持久化 Canvas metadata 与源码文件，没有 program definition/execution。

### 浏览器提供了临时编排效果

- `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.runtime.ts` 将 TS/TSX transpile 为 Blob ES module，并在 iframe 中 dynamic import entry。
- boot SDK 暴露 `window.agentdash.invoke(actionKey,input)`、`extensions.invoke(...)`、interaction state 与 `agent.submit`。
- 普通 JavaScript 的 `await` / `Promise.all` 负责顺序、分支和并发；后端每次只收到一个 `{ action_key, input }` invocation。

当前不存在 program parse、bind、preflight、execution id、root cancellation、root trace 或 durable step result。

### Agent 没有执行入口

- Workspace Module 为 Agent 提供 Canvas create/attach/present/VFS/bind/inspect/get-interaction-state 等能力，但没有 `run`。
- Agent 可以写入 Canvas 源码，让用户之后在浏览器运行；不能 headless 执行其中的 JavaScript 编排。
- 普通 Canvas 未注入 `extensionChannelBridge`；只有 `ExtensionCanvasPanel` 能直接调用 Extension protocol channel。因此“任意 Canvas 已能编排 Extension protocol”也不成立。

## 3. 现有 Operation 与 admission 基础

可复用基础已经存在：

- `RuntimeActor` 已区分 `AgentSession`、`UserCanvas`、`SessionUser`、`WorkflowNode` 等 actor。
- RuntimeGateway 会验证 actor/context，并由 provider 暴露 action surface。
- MCP runtime provider 已有 `mcp.list_tools`、`mcp.call_tool`，且 MCP discovery 会解析当前 session capability-filtered surface。
- Workspace Module 已把 Extension operation catalog 规范化为 `module_id + operation_key + schema + visibility + dispatch + provenance`。

但不能把一次外层 program tool 获准等价为所有内部步骤获准：

- Agent 普通 tool call 的 `before_tool_call`、审批与 `after_tool_call` 位于 agent loop；program 若直接调用 RuntimeGateway 会绕开它。
- `RuntimePolicy.required_capabilities/timeout_ms/allow_background` 当前主要是 descriptor，未形成统一 gateway enforcement。
- MCP runtime call 创建未关联的 cancellation token；Canvas 60 秒 timeout 只拒绝浏览器 Promise，不取消后端工作。
- 每次 invocation 有自己的 trace，却没有 program root/step parent trace。
- Canvas、Extension bridge 与 Agent tool 的大结果裁剪/result-ref 行为不一致。

因此需要在 RuntimeGateway 内收束一个供 direct invocation 与 program step 共用的 `OperationExecutionCore`，统一 actor-neutral 的 schema/capability admission、执行、取消、结果与 trace。Agent loop 的 message/tool-call hook 与用户审批仍留在 Agent 外层；Program 通过静态 effect manifest/digest 接受一次外层审批，step 只能执行该 manifest 内的 operation，并在运行时重新校验当前 capability。

## 4. Canvas 资产、运行与展示的当前耦合

### Definition / source

`Canvas` 聚合同时保存资产身份、Project、发布谱系、sandbox/import map 与所有源文件。Project 内 `mount_id` 又被派生为 Workspace Module ref、VFS mount、presentation URI 和 AgentRun state key。

AgentFrame 只固定“挂载哪个 Canvas 与什么权限”，VFS provider 每次重新读取 repository；它不固定 Canvas source revision。Canvas 更新也没有 revision/CAS，多个入口会整份覆盖 source files。

### Runtime instance 实际不存在

`CanvasRuntimeSnapshot` 是临时组装 DTO，没有 runtime instance ID 或 definition revision。浏览器 `frame_id + generation` 是真正执行 JS 的 renderer instance，但后端没有相应实体或 lease。

Canvas attach/present 最终写入 AgentFrame revision 的 VFS mount、visible canvas 和 module projections。这应被解释为 Agent 对 Canvas 的 capability attachment，不是 Canvas runtime instance。

### Interaction state 只是遥测快照

- `agent_run_canvas_runtime_observations` 与 `agent_run_canvas_interaction_snapshots` 均以 `run_id + agent_id + canvas_mount_id` 唯一。
- iframe reload 时不会从后端 snapshot 回灌 canonical state。
- upsert 无 expected revision/generation guard；多 tab 或协作者是 last-write-wins。
- recent interaction events 只是 snapshot 内最多 20 条本地事件，不是 append-only log。
- Agent 只有 `canvas.get_interaction_state` 读取，没有 command、patch 或 subscription。

因此当前状态属于“浏览器向 Agent 上报的 latest observation”，不属于双方共同维护的 runtime state。

### Presentation

WorkspacePanel tab/layout 是按 AgentRun key 保存的用户展示偏好；浏览器 tab instance 也是临时 ID。这一层不应拥有 Canvas attachment 或 InteractionInstance 生命周期。

## 5. 已确认的存量合同断口

后端 `crates/agentdash-api/src/routes/canvases.rs` 已明确删除 `/canvases/{id}/runtime-snapshot`，只保留 AgentRun scoped runtime routes；前端 `packages/app-web/src/services/canvas.ts` 仍请求旧 endpoint，`ProjectCanvasManager.tsx` 在没有 AgentRun bridge 时仍挂载该路径。

结果是普通 Project Canvas 预览/保存后刷新会请求不存在的 API。这个断口应作为后续实现的先修项：明确拆分静态 asset preview 与 attached Interaction runtime preview，而不是恢复已经删除的 Session 兼容路由。

另一个断口是 `CanvasAgentInputSubmitRequest` 虽携带 interaction snapshot/render observation IDs，API 构造 mailbox command 时没有消费它们；当前 `include_interaction_state/include_render_observation` 并未实现文档承诺的上下文注入。

## 6. Extension UI 当前能力

- manifest UI 合同只有 `workspace_tabs[]`，renderer 为完整 `webview` 或 `canvas_panel`。
- Canvas promotion 生成的是完整 CanvasPanel snapshot，不是可嵌套组件。
- `defineApp()` 只有单个 panel entry；React package 只重导 browser bridge。
- 没有 component key、props/events schema、slots/layout、component instance、shared-state binding 或 component-scoped capabilities。
- Extension browser `events.emit()` 只在 iframe 内触发 local subscriber，宿主不会把它变成 interaction event。

当前 Canvas iframe 使用 `allow-scripts allow-same-origin` 与全局 `postMessage("*")`，Extension asset response 也没有 component-specific CSP。把第三方 ESM/Web Component 直接导入同一 realm 会让所有组件共享 `window.agentdash`，无法形成真正的 capability membrane。

## 7. 三类组件 ABI 对照

| ABI | 优势 | 主要限制 | 定位 |
| --- | --- | --- | --- |
| Schema-driven host renderer | theme/a11y/layout 一致，状态与权限完全由宿主管理 | 无法表达任意创新 UI | 宿主 primitive tier |
| Web Components / ESM | framework-neutral tag，嵌套和布局自然 | 同 realm、全局 registry/版本冲突、难以撤权与隔离 | 未来可信/签名组件 tier |
| Isolated iframe component | framework/CSS/全局变量隔离；可用 MessagePort 注入最小能力 | 需要 sizing、focus、event 与性能协议 | 第三方任意 UI 的推荐 MVP |

首个可验证方案应是“声明式 component descriptor + isolated iframe renderer”。这不是继续使用整页 tab：component 具有自己的 key、props/events/commands/state projection、layout/sizing 与 instance-scoped capability，并可被 Canvas layout 中的 slot 引用。

## 8. 现状对目标模型的约束

- Workspace Module 应继续做 Agent-facing discovery/projection，不成为 Operation、Canvas、Interaction 或 Extension manifest 的第二事实源。
- Project/installation/backend/session 都仍有必要，但只在 attachment/resolution/invocation 层出现，不能成为 InteractionInstance identity。
- AgentFrame 继续表达某个 Agent 当前有效 capability revision。
- RuntimeSession 当前拥有 live turn、event、resume、delivery 与 trace substrate，但不拥有 interaction identity/state；这是存量 plumbing 事实，不是目标保留边界。已确认的目标是让 RuntimeGateway、Canvas 与 Extension 不再要求 RuntimeSession，renderer refresh 只替换 lease，不销毁 shared state。
- Channel 只承担 attention、notification、mailbox/external delivery，不拥有 interaction attachment、command/event/state。

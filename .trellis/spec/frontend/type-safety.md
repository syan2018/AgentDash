# Type Safety

> 前端类型安全规范。

---

## 核心原则

- **严格模式**：TypeScript strict 已启用，禁止 `any`、类型断言（`as`）、非空断言（`!`）
- **snake_case 直接映射**：前端类型字段名与后端 Rust DTO 直接对齐，不做 camelCase 转换
- **Generated wire 单源**：内部 API 响应通过 `src/generated/*` 的 contract type 消费，service 层信任 generated wire，不做逐字段 identity rebuild

---

## 类型分层

| 位置 | 用途 |
|------|------|
| `types/index.ts` + 拆分文件 | 跨 Feature 共享的领域类型 |
| `features/{name}/model/types.ts` | Feature 内部类型 |
| `generated/backbone-protocol.ts` | 自动生成的协议类型，禁止手动修改 |
| `generated/*-contracts.ts` | Rust contract crate 生成的 HTTP / NDJSON DTO，作为跨层 wire type 来源 |

---

## Mapper 边界

mapper 只负责：
- UI view model 转换
- 外部/用户输入、第三方 payload、iframe/plugin bridge 等非内部 API 边界的 `unknown → typed object` 转换
- 尚未进入 contract crate 的 route-local 过渡 DTO

mapper 不负责：
- 同时兼容 `camelCase` + `snake_case`（出现 `fooBar ?? foo_bar` 时应修后端 DTO）
- 猜测后端字段别名
- 重新声明后端 enum/string union；跨层 DTO 的联合类型来自 `src/generated/*`
- 对 generated DTO 做逐字段 identity rebuild

## Generated Contract Boundary

前端把 `src/generated/*` 当作 wire DTO 事实源。Feature 可以定义 view model，但 view model 必须由 generated DTO 显式转换而来，原因是 UI 形态与 transport 形态有不同变化节奏。

Canvas 资产 UI 消费 `generated/canvas-contracts.ts` 中的 `CanvasResponse`、`CanvasScopeDto`、`CanvasAccessDto`、`CanvasListScopeDto`、`PublishCanvasToProjectRequest`、`CopyCanvasToPersonalRequest` 和 `UnpublishCanvasResponse`。`services/canvas.ts` 只封装 endpoint 和 query/body 传递；Mine/Shared 分组、按钮可见性和 editor 只读状态全部读取 `canvas.scope` 与 `canvas.access`。

Canvas access-driven UI contract:

| `CanvasResponse.access` field | Frontend behavior |
| --- | --- |
| `can_edit_source` | 显示源文件/绑定保存入口，允许 `updateCanvas` |
| `can_publish` | 显示“发布到项目共用”；“发布为插件”仍是独立 action |
| `can_copy` | Shared 视图显示“复制为我的 Canvas” |
| `can_manage_shared` | Shared 视图显示取消发布/删除共用源 |
| `runtime_write_allowed=false` | runtime preview 保持可用，source editor 以只读状态展示或禁用 |

Validation:

- `handleBindingsSave`、source file save 等 mutation handler 在调用 `updateCanvas` 前检查 `canvas.access.can_edit_source`，原因是 UI disabled state 不是权限边界。
- 复制 shared Canvas 成功后，UI 刷新列表、切到 Mine 并选中新 personal Canvas；后续编辑只作用于 clone。
- 前端不从 `owner_user_id`、Project role 或当前用户缓存推导编辑权限；这些事实已经由后端合并为 `access`。

Project extension runtime surface 消费 `generated/extension-runtime-contracts.ts`，`services/extensionRuntime.ts` 只保留 endpoint 调用与 webview asset URL 拼装。`features/extension-runtime` 以 Project ID 为 key 缓存 runtime projection，并向 WorkspacePanel 输出 tab descriptor 与 webview bridge；installation 的 `installed_source` 与 `package_artifact` 是显式可空字段，用来区分 Shared Library 安装来源与 packaged artifact 安装来源；前端不从 Shared Library payload 或 Session Context 推断 extension runtime 声明。

Extension webview bridge 的 `runtime.invoke_action` 只校验 Project、Session、backend 与 action key 这些宿主上下文，并把 `action_key + input` 交给后端 RuntimeGateway，原因是具体 action 是否在当前 actor/context 下可执行由 Gateway catalog / invoke 同源裁决。Project extension runtime projection 的 `runtime_actions` 服务资产展示，不作为前端执行可用性 gate。

新增或修改跨层 DTO 时同步运行：

```powershell
pnpm run contracts:check
```

---

## CapabilityDirective 契约

`CapabilityDirective` 使用 qualified path 字符串（`{ add: string } | { remove: string }`），支持能力级、工具级、MCP 能力。`CapabilityKey` 仅用于前端内置能力选项的 UI 展示，不要用它收窄 API 配置中的 `capability_directives`。

## Session Runtime Projection DTO

Session workspace panel、context overview 与 VFS tab 以 `runtime_surface: ResolvedVfsSurface` 作为运行时 mount 展示、默认 mount、可浏览性与编辑能力的唯一 UI 输入。`ExecutionVfs` 保留为 session context DTO 中的 runtime 诊断信息；界面读取 final projection DTO，可以保证 pending runtime patch、VFS overlay 与后端 capability projection 完成后，前端展示的是最终生效的地址空间。

Project / Story / Task / Agent knowledge 等预览入口使用 `ResolvedVfsSurfaceSource` 解析 preview surface；Session 入口直接消费 `session_runtime` 的 `runtime_surface`。两类入口共享 VFS browser 组件，但各自的 surface 来源显式表达，方便在跨层测试里验证 preview 与 runtime 语义。

Session 右侧 WorkspacePanel 消费 current runtime projection state。该 state 以 `runtime_session_id + frame/runtime projection key` 为边界，携带 loading / ready / refreshing / error 状态；key 不匹配时不暴露上一份 runtime surface、capabilities 或 context snapshot。`workspace_module_presented`、`capability_state_changed` 等事件只触发当前 state 的 invalidate/refetch，界面不创建新的长期快照事实源。Canvas 打开动作读取 generated event payload 的 `presentation_uri`，值为 `canvas://{canvas_mount_id}`；`view_key`、`module_id` 与 `{canvas_mount_id}://...` 分别保留 UI entry selection、module ref 与 VFS authoring URI 语义。

## AgentRun Conversation DTO

AgentRun workspace 消费 `AgentConversationSnapshot` / `AgentRunWorkspaceView.conversation` 的 generated DTO。输入区、pending row、model selector 与 keyboard submit 使用 `ConversationCommandSetView.commands`、
`ConversationKeyboardMapView`、`ConversationModelConfigView` 和 `ConversationPendingSnapshotView`，原因是这些字段携带后端同一轮 snapshot 的 command id、stale guard、模型解析和用户注意力语义。

AgentRun command handlers 以 `ConversationCommandView.enabled`、`unavailable_reason` 和 `commandPrecondition(command)` 作为 mutating command 的语义准入来源；`workspace_status`、`delivery_status` 与 workspace projection loading state 只服务展示和刷新 UX。这样做的原因是后端 command resolver 与 command policy 共享 stale guard，前端如果再用 workspace status 派生 allow/deny 会绕开同源 command contract。

ProjectAgent draft start 使用 generated `CreateProjectAgentRunRequest` / `ProjectAgentRunStartResult`。启动成功后前端只用 `run_ref` / `agent_ref` 导航并刷新 AgentRun workspace；首轮输入是否 queued/launched/failed 由 `initial_message: AgentRunMessageCommandResponse` 和后续 workspace/mailbox projection 表达。前端不从 `runtime_session_id`、可选 `turn_id` 或 HTTP success 派生聊天投递状态，原因是 draft workspace materialization 与 connector accepted 是不同边界。

AgentRun 右侧 WorkspacePanel 使用 snapshot `resource_surface: ResolvedVfsSurface`。该 surface 来自 AgentRun 当前 frame 的 typed VFS surface，并由后端 AgentRun surface resolver 叠加 `RuntimeSessionExecutionAnchor` 锚定的 `agent_run_session` lifecycle mount；RuntimeSession detail 仍可以用 `ResolvedVfsSurfaceSource::SessionRuntime` 展示 trace/detail 视角。两条入口共享 browser 组件，但 AgentRun producer 是 snapshot resource surface，原因是 AgentRun workspace 需要浏览当前 delivery session 的执行证据，而不是数据库层 run inventory。

---

## Task Plan And Story Projection DTO

Task plan DTO、Story Task projection DTO 与 Task status enum 都来自 Rust contract 生成文件。前端只消费 generated plan status union；execution status、artifacts 和 launch hint 字段由各自的 generated DTO 表达。

AgentRun workspace 消费 Run-scoped Task plan DTO 来创建、推进、归档和 assignment。Story 页面消费 Story Task projection DTO，只展示来源关系；runtime artifacts、latest runtime node 和 linked runs 只从 `SubjectExecutionView` / lifecycle generated DTO 读取。

新增或修改 Task plan / projection contract 后必须运行：

```powershell
pnpm run contracts:check
pnpm run frontend:check
```

---

## 禁止模式

- `any` 类型
- `as SomeType` 类型断言（除非编译器无法推断的极少数场景）
- `value!` 非空断言
- 为 generated DTO 编写逐字段 identity mapper

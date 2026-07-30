# Legacy Canvas 能力基线

## 对照源

- 只读工作树：`D:/ABCTools_Dev/AgentDash-main-reference`
- commit：`957fa9d60ea3d67efa1bb278fe5b376cf0c34598`
- remote ref：`origin/codex/main-reference-backup-20260725`
- 时间点：2026-07-09，早于 `3028f4456` 的 Canvas→Interaction 删除提交

该工作树是迁移验收基线，不是待恢复的数据模型。验收以“旧产品能力是否仍可完成”为准，
不以旧类型、旧路由或旧 API 名是否存在为准。

## 完整文件面

后端能力集中在：

- `crates/agentdash-workspace-module/src/canvas/`
- `crates/agentdash-workspace-module/src/workspace_module/`
- `crates/agentdash-api/src/routes/canvases.rs`
- `crates/agentdash-application/src/canvas/`
- `crates/agentdash-domain/src/canvas/`
- `crates/agentdash-application-runtime-gateway/`

前端能力集中在：

- `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.runtime.ts`
- `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.tsx`
- `packages/app-web/src/features/canvas-panel/CanvasFilesEditor.tsx`
- `packages/app-web/src/features/canvas-panel/CanvasRuntimeBindingsEditor.tsx`
- `packages/app-web/src/features/canvas-panel/ProjectCanvasManager.tsx`

Agent 使用合同集中在：

- `crates/agentdash-domain/src/canvas/skills/canvas-system/`

逐工具的名称、参数、返回、错误、权限和副作用见
`research/legacy-tool-behavior-matrix.md`；该矩阵是 Agent-facing parity 的直接验收账本。

## 能力矩阵

| 能力 | Legacy 行为 | 新实现必须保留的产品结果 |
| --- | --- | --- |
| Canvas 资产管理 | 个人/项目共用列表、创建、读取、更新、删除 | 用户可独立管理个人与项目 Canvas |
| 分发 | 发布到项目、复制为个人、取消发布 | 编辑权与共享权清晰，复制来源可追踪 |
| Extension 晋升 | Canvas source 打包并安装为 Extension | 可从确定 revision 晋升，不依赖旧 aggregate |
| 多文件创作 | `src/main.tsx`、TS/TSX/JS/JSX/CSS/JSON、本地 import | SourceBundle 多文件编辑与预览保持完整 |
| Agent 创作 | `workspace_module_operate` 的 create/copy/attach、VFS read/list/search/write/apply patch | Agent-facing 工具保持旧合同，可从空项目创建并持续编辑 Canvas |
| AgentFrame 纳入 | create/copy 创建后纳入，attach 将既有 Canvas 纳入 | create/copy 复用 create 链，attach 使用 attach 链；module ref 与 VFS mount 经 immutable Frame + Product surface 收敛 |
| VFS 地址 | `{mount_id}://...`，无 exec | 保留稳定、可描述的 source mount 使用体验 |
| 数据绑定 | `canvas.bind_data` 生成只读 `bindings/*` | 用 resource slot/binding 保留同等数据注入能力 |
| 展示 | `workspace_module_present` 打开 Canvas preview | Canvas/Interaction presentation 可交付给用户，且 present 不修改 AgentFrame |
| Runtime capability | `window.agentdash.invoke(actionKey, input)` | Canvas 内用户动作可调用当前授权能力并显示结果 |
| Extension channel | `window.agentdash.extensions.invoke(channelKey, method, ...)` | Canvas 可调用当前授权的 Extension protocol/backend capability |
| VFS asset | `assets.url/revoke` | VFS 图片/二进制可安全转换为可撤销浏览器 URL |
| Interaction state | `setState/clearState/emit/getState` | Canvas 可发布 Agent 可观察的选择、表单、过滤器与最近事件 |
| Agent 检查 | `canvas.inspect`、`canvas.get_interaction_state` | Agent 可读取最新渲染诊断与 allowlisted 交互状态 |
| Agent submit | `agent.submit`，支持 queue/steer | attached Canvas 可向 AgentRun mailbox 投递用户输入 |
| Render observation | ready/error、viewport、DOM summary、diagnostics | 预览失败和当前渲染状态可被可靠诊断 |
| Runtime generation | frame id、generation、请求/结果关联、卸载清理 | reload/reconnect/stale message 不串代 |
| 安全边界 | iframe 不持有 token、backend/session id、本机路径 | 新 SDK 继续只接收最小投影与 opaque handle |

## Legacy Gateway 中值得保留的机制

- 宿主绑定 actor、project/session 上下文，iframe 只提交 capability identity 与 input。
- 请求有 request id、frame id、generation，结果必须回到同一代 iframe。
- capability surface 先发现后调用，宿主拒绝当前 surface 不可见的调用。
- action/extension channel 调用、资源解析、交互快照、诊断与 Agent submit 是不同消息种类。
- asset URL 有 cache 与显式/卸载 revoke。
- interaction snapshot 与 render observation 是 Agent 可读事实，但不自动进入对话历史。
- submit-to-Agent 只由显式用户动作触发，并复用 mailbox command receipt。

## 最终实现中不保留的 Legacy 内部

这些文件仍在第一阶段原样硬搬，用于保全行为与测试；完成新底层适配后才删除：

- mutable `Canvas` aggregate 与 `CanvasRepository`
- `canvas_fs` 作为 source 事实源
- `RuntimeSession` 作为 Canvas bridge owner
- `RuntimeGateway` action-key 执行权威
- AgentRun-scoped Canvas runtime snapshot/state 表
- `window.agentdash.invoke(actionKey, input)` 字符串执行协议
- 旧 HTTP route/DTO/generated contract

这些内部被替换，不构成能力删减。

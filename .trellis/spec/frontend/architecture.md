# Frontend Architecture

## Role

前端负责以 Project 为中心组织业务视图，消费后端权威状态与实时事件，提供 Workspace、Story、Task、Session、Workflow、VFS、Assets 等交互界面。前端不创建第二套业务事实源。

## Invariants

- API 字段直接使用后端 `snake_case`，前端不做 camelCase/snake_case 双风格兼容。
- API 响应必须经 mapper 从 `unknown` 转换为 typed object。
- Story / Task / Session / Workflow 等业务状态以后端为准，前端不自行推断权威状态。
- Lifecycle 运行态以后端 `LifecycleRunView` / `SubjectExecutionView` / `AgentFrameRuntimeView` 为准；`RuntimeSession` 页面只展示 trace，不作为业务执行归属事实源。
- Project 是顶层导航和隔离单元；Workspace、Story、Assets、runtime preview 都按 Project scope 组织。
- Session workspace panel、context overview 和 VFS tab 以 `runtime_surface` 作为 runtime mount 展示与浏览能力的唯一 UI 输入。
- Feature module 遵循 model / ui 分离，跨 feature 共享能力进入明确的 shared package 或 primitive。
- Workspace tab、runtime data context 和 tab descriptor contract 放在 `features/workspace-runtime`，原因是 extension-runtime、workspace-panel 与 canvas-panel 都需要消费同一 workspace runtime surface，但不应形成 feature 间双向依赖。

## Current Baseline

主要包：

| Package | 当前职责 |
| --- | --- |
| `packages/app-web` | React Web 主应用 |
| `packages/app-tauri` | Tauri 桌面入口 |
| `packages/ui` | 共享 UI primitive 与样式 |
| `packages/core` | 共享核心逻辑与 ports |
| `packages/views` | 可复用 view components |
| `packages/extension-sdk` | Extension host 侧插件作者 API 与 contribution collector |
| `packages/extension-ui` | Workspace webview panel 内的 extension bridge API |
| `packages/extension-dev` | Extension authoring CLI，负责 init / dev / validate / pack / install |

主应用组织：`api/`、`services/`、`stores/`、`features/<feature>/model`、`features/<feature>/ui`、`pages/`、`types/`、`generated/`。

## Local Decisions

- 前端类型直接使用 `snake_case`，原因是它让 DTO 契约错误暴露在 mapper / typecheck 边界，而不是被双读字段掩盖。
- 设计系统优先使用 `@agentdash/ui` primitive，原因是重复业务布局会让视觉语言和交互状态持续漂移。
- 长连接统一使用 fetch + ReadableStream 消费 NDJSON，原因是鉴权、resume、HMR cleanup 需要与普通 API 和 stream registry 对齐。
- Extension authoring surface 使用独立 `packages/extension-*` 工作区包，原因是插件作者 API、webview bridge 与开发 CLI 需要随插件协议版本收敛成窄接口。
- WorkspacePanel 的插件 tab 由 `features/extension-runtime` 消费 Project scoped runtime projection 后注册，原因是插件 catalog 是 Project enabled installation 的全局视图，不应随单个 session 生命周期被创建或销毁。
- Extension webview action target 优先使用 Session runtime surface backend，缺省时使用当前 Project workspace binding，原因是 WorkspacePanel 插件 tab 的生命周期归属 Project，而本机 extension host 的可执行 backend 来自 workspace 授权事实。
- `canvas_panel` 插件 tab 在主前端读取 package artifact 内的 Canvas runtime snapshot 并复用 `CanvasRuntimePreview`，原因是 Canvas-derived extension 需要沿用 Canvas runtime sandbox/asset bridge，同时保持 Project extension installation 作为 WorkspacePanel tab catalog 的事实源。
- `@agentdash/extension-ui` 的 webview bridge 只让 panel 传递 method 与 JSON params；Project、session、backend、consumer extension 和 trace context 由 `ExtensionWebviewPanel` 组装，原因是 panel 运行在 iframe 中，不应成为 Project runtime routing 的事实源。
- Extension panel 的 bridge request surface 包含 `metadata.get_context`、`workspace.open_tab`、`runtime.invoke_action`、`extension.invoke_channel`、`vfs.read` 和 `vfs.write`；`events` 是 panel-local event bus，原因是 workspace-level 或 extension-runtime-level event 需要后端路由和订阅模型，不能混入本地 helper。
- Canvas runtime 如需消费 extension protocol channel，通过父页面注入的 `extensionChannelBridge` 进入同一 Project extension channel invocation service，原因是 Canvas 与 webview panel 都应依赖 Project runtime projection 和 Gateway admission，而不是在 iframe 里硬编码 provider extension key。
- Assets Extension 类目消费 Project extension management API，原因是安装、来源状态、package mode 与卸载/下载动作的事实源是 `ProjectExtensionInstallation`，runtime projection 只服务 WorkspacePanel 与 Gateway admission。
- Marketplace Extension 卡片和详情抽屉使用 `LibraryAssetDto.extension_package_artifact` 判断 packaged template 可安装性，原因是浏览、安装与发布后的 package 可用状态需要共享同一 Shared Library 合同。
- WorkspacePanel 是 extension/canvas tab 的 composition root；extension-runtime 与 canvas-panel 不反向依赖 workspace-panel，原因是插件 tab 注册、Canvas 预览和 workspace runtime context 需要保持单向装配关系。
- Workflow 资产入口是 `WorkflowGraph` 定义态入口；Agent Activity 关联的 `AgentProcedure` contract 可以作为编辑器配套 draft 一起维护。运行态观察进入 `lifecycleStore`，原因是 graph definition 与 lifecycle projection 的变化节奏不同。

## Contract Appendices

- [Directory Structure](./directory-structure.md)
- [Type Safety](./type-safety.md)
- [State Management](./state-management.md)
- [Hook Guidelines](./hook-guidelines.md)
- [Component Guidelines](./component-guidelines.md)
- [Design Language](./design-language.md)
- [Quality Guidelines](./quality-guidelines.md)
- [Activity Lifecycle Frontend Contract](./workflow-activity-lifecycle.md)

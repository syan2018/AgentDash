# Frontend Architecture

## Role

前端负责以 Project 为中心组织业务视图，消费后端权威状态与实时事件，提供 Workspace、Story、Task、Session、Workflow、VFS、Assets 等交互界面。前端不创建第二套业务事实源。

## Invariants

- API 字段直接使用后端 `snake_case`，前端不做 camelCase/snake_case 双风格兼容。
- API 响应必须经 mapper 从 `unknown` 转换为 typed object。
- Story / Task / Session / Workflow 等业务状态以后端为准，前端不自行推断权威状态。
- Project 是顶层导航和隔离单元；Workspace、Story、Assets、runtime preview 都按 Project scope 组织。
- Session workspace panel、context overview 和 VFS tab 以 `runtime_surface` 作为 runtime mount 展示与浏览能力的唯一 UI 输入。
- Feature module 遵循 model / ui 分离，跨 feature 共享能力进入明确的 shared package 或 primitive。

## Current Baseline

主要包：

| Package | 当前职责 |
| --- | --- |
| `packages/app-web` | React Web 主应用 |
| `packages/app-tauri` | Tauri 桌面入口 |
| `packages/ui` | 共享 UI primitive 与样式 |
| `packages/core` | 共享核心逻辑与 ports |
| `packages/views` | 可复用 view components |

主应用组织：`api/`、`services/`、`stores/`、`features/<feature>/model`、`features/<feature>/ui`、`pages/`、`types/`、`generated/`。

## Local Decisions

- 前端类型直接使用 `snake_case`，原因是它让 DTO 契约错误暴露在 mapper / typecheck 边界，而不是被双读字段掩盖。
- 设计系统优先使用 `@agentdash/ui` primitive，原因是重复业务布局会让视觉语言和交互状态持续漂移。
- 长连接统一使用 fetch + ReadableStream 消费 NDJSON，原因是鉴权、resume、HMR cleanup 需要与普通 API 和 stream registry 对齐。

## Contract Appendices

- [Directory Structure](./directory-structure.md)
- [Type Safety](./type-safety.md)
- [State Management](./state-management.md)
- [Hook Guidelines](./hook-guidelines.md)
- [Component Guidelines](./component-guidelines.md)
- [Design Language](./design-language.md)
- [Quality Guidelines](./quality-guidelines.md)
- [Activity Lifecycle Frontend Contract](./workflow-activity-lifecycle.md)


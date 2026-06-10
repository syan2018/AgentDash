# 评估并收束 Canvas 工具到 workspace_module

## Goal

把 Agent-facing 的 Canvas 专用工具面收束到 `workspace_module` 体系中，让 Agent 通过单一模块目录发现、描述、调用和展示 Canvas / Extension / Builtin workspace 能力。

本任务的核心价值是消除当前“双工具面”带来的语义分裂：Canvas 已经作为 `canvas:{mount_id}` 出现在 workspace module projection 中，但创建、接入、绑定、展示和 session 暴露仍由独立 `canvas` capability 下的 `canvases_list` / `canvas_start` / `bind_canvas_data` / `present_canvas` 承担，导致模块可见性、工具默认面和前端展示事件无法形成一个闭环。

## Confirmed Facts

- `workspace_module` 聚合层已经把 enabled extension 与 project canvas 投影成统一 `WorkspaceModuleDescriptor`，Canvas module 当前只有 UI entry，没有 invokable operation。
- `WorkspaceModuleOperationDispatch` 已预留 Canvas 分支，但现有 Canvas 工具执行的是 host-owned application operation，而不是 Canvas runtime action。
- `present_canvas` 会把 Canvas 暴露到当前 session：追加 VFS mount、写入 visible canvas mount、刷新 capability state，然后发 `canvas_presented`。
- `workspace_module_present` 当前只发 `workspace_module_presented` 事件，不执行 Canvas session 暴露逻辑。
- capability catalog 与 tool provider 仍把 `canvas` 和 `workspace_module` 作为两个并列 well-known capability；默认 session plan 仍列出 Canvas 专用工具。
- ProjectAgent 配置落在 `project_agents.config` JSON；需要通过 forward migration 处理已有 `capability_directives` 中的 `canvas` 工具能力意图。

## Requirements

1. `workspace_module` 成为 Agent 调用 Canvas 能力的唯一主入口，覆盖发现、描述、调用、展示四类 Agent-facing 操作。
2. Canvas 创建/接入、数据绑定必须作为 workspace module operation 暴露，并统一通过 `workspace_module_invoke` 执行；实现可以复用 Canvas application use case，但 Agent 不再需要直接调用 Canvas 专用工具。
3. Canvas 展示必须走 `workspace_module_present`。它对 Canvas renderer 不能只是发 UI 事件，还必须与现有 `present_canvas` 语义一致：目标 Canvas 对当前 session 可见、runtime VFS/capability state 已刷新、前端可打开对应 Canvas tab。
4. 新的 operation dispatch 必须表达 host-owned Canvas operation，避免把 Canvas 资产管理误建模成 extension runtime action 或 iframe runtime action。
5. 能力目录、默认 session plan、tool provider、前端 capability picker 应收束到 `workspace_module`，不继续把 `canvas` 作为普通 Agent 可选工具能力暴露。
6. 数据库迁移采用 forward migration，处理 `project_agents.config` 中已有 `capability_directives` 的 `canvas` → `workspace_module` 转换；Canvas 资产表和文件/绑定数据保持原业务事实源。
7. 前端 WorkspacePanel 仍复用 Canvas renderer/panel，但触发入口统一来自 `workspace_module_presented`，并使用明确的 Canvas presentation URI。
8. 增加或更新 Agent-facing skill 指南，说明 `workspace_module_*` 的正确使用流程、Canvas 迁移后的入口选择、`list -> describe -> invoke/present` 的调用顺序，以及何时再加载 `canvas-system`。
9. 任务完成后，相关 spec 需要说明为什么 Canvas 资产管理通过 Workspace Module 暴露给 Agent，而 Canvas domain / VFS / panel 仍保持内部独立边界。

## Non-Goals

- 不重写 Canvas runtime sandbox、preview renderer 或 Canvas 文件存储模型。
- 不把 Canvas 资产表合并进 extension package installation。
- 不建设完整 Project Settings 管理台；本任务只处理 Agent-facing tool surface 与展示闭环。
- 不保留两套等价 Agent 工具作为长期兼容路径。本项目处于预研期，正确收束优先。
- 不把 workspace module skill 写成内部架构文档。skill 只描述 Agent 调用协议和必要边界，细节仍由工具 schema、describe payload 和 Trellis spec 承担。

## Acceptance Criteria

- [ ] `workspace_module_describe` 能对 Canvas host module / Canvas instance module 返回可调用 operations，至少覆盖 create-or-attach、bind-data；Canvas instance module 同时返回可展示 UI entry。
- [ ] `workspace_module_present` 展示 Canvas 后，session runtime surface 能看到对应 Canvas VFS mount，能力状态刷新事件先于展示事件发出。
- [ ] 前端收到 Canvas 类型的 `workspace_module_presented` 后能稳定打开 `canvas://{mount_id}` 对应 tab，不依赖旧 `canvas_presented` 或 `activeCanvasId` 旁路。
- [ ] 默认 Agent 工具面不再注入 `canvases_list` / `canvas_start` / `bind_canvas_data` / `present_canvas` 作为独立主入口；默认工具说明指向 `workspace_module_*`。
- [ ] capability catalog / picker 中 Canvas 相关 Agent 能力通过 `workspace_module` 表达；`canvas` 不再作为普通 well-known Agent capability 出现。
- [ ] forward migration 能把已保存 ProjectAgent config 中的 `canvas` capability directive 改写为 `workspace_module`，并通过 migration guard。
- [ ] 新增或更新内嵌 `workspace-module-system` skill，并在 session 具备 `workspace_module` capability 时可被 Agent 发现；skill 内容通过 skill validation。
- [ ] `canvas-system` 的核心流程不再要求直接调用旧 Canvas 工具，而是指向 workspace module 创建/展示入口后再进行 Canvas 源码编辑。
- [ ] 后端针对 workspace module Canvas operations、present session 暴露、capability filtering 有单元测试或集成测试覆盖。
- [ ] 前端针对 Canvas workspace module present URI 解析和 tab 打开逻辑有 focused 测试或等价验证。
- [ ] 相关 Trellis spec 更新完成，记录当前目标架构的原因和边界。

## Open Decisions

- 当前建议是一次性硬切 `canvas` Agent capability 到 `workspace_module`。除非后续评审发现第三方集成必须短期并存，否则实现按硬切处理。

## Notes

- 本任务已完成初步评估，属于复杂任务；进入实现前需要 review `design.md` 和 `implement.md`。

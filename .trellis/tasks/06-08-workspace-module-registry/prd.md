# PRD · Agent Workspace Module Registry 与 Canvas Extension 协作面预研

## 背景

当前 TS Extension Host、Project extension installation、RuntimeGateway、WorkspacePanel 动态 tab、Canvas promote extension 已经形成可运行链路，但 Agent 获取插件接口的路径仍不够明确：

- 前端可以通过 Project scoped `extension_runtime` projection 发现 `runtime_actions`、`protocol_channels`、`workspace_tabs`。
- RuntimeGateway 可以在调用阶段按 Project enabled extension installation 路由 extension action。
- Agent prompt 前实际获得的是 `context.turn.assembled_tools`，当前主要来自平台内置工具、direct MCP、relay MCP。
- Canvas 既是项目级可视化资产，也已经可以发布为 extension package；从协作模型看，它更像 Agent 动态创建、使用公共信道运行的前端 extension instance。

前端 GUI 协作层已经围绕 Workspace 命名：`WorkspacePanel` 是动态 tab 容器，`WorkspaceRuntimeData` 是工作台运行上下文，Extension 通过 `workspace_tabs` 转成 `TabTypeDescriptor` 注册到 workspace tab registry，Canvas 也在 WorkspacePanel 中以 `canvas://...` tab 打开。因此本任务将 Agent 可见的 GUI/接口协作模块命名为 **Workspace Module**，而不是把顶层协作入口继续称为 Surface。

## 目标

建立一份可供后续实现任务引用的规划底座，明确：

- Workspace Module 的定义、边界和事实源。
- Extension、Canvas、Protocol Channel、内置平台能力如何统一投影给 Agent。
- Agent 应使用少量稳定 workspace module tools 发现和操作协作模块，而不是把每个 extension action 展开成独立 Agent tool。
- Canvas 作为动态 workspace module instance 的概念位置，以及它与 packaged extension 的关系。
- Workspace Module 与底层 Runtime Surface 的术语分界。
- 后续可拆分的实现任务、验收路径和风险点。

## 已确认事实

- `docs/extension-system.md` 将 `runtime_actions` 定义为 AgentDash runtime 可直接调用的 action surface；`protocol_channels` 是插件导出的 provider API surface。
- `crates/agentdash-application/src/extension_runtime.rs` 已能从 enabled Project extension installations 聚合 `ExtensionRuntimeProjection`，包含 installations、runtime actions、protocol channels、dependencies、workspace tabs、permissions、bundles。
- `crates/agentdash-api/src/routes/extension_runtime.rs` 暴露 `GET /projects/{project_id}/extension-runtime`，前端 `SessionPage` 已按 owner Project 读取 projection。
- `packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx` 注册 Extension tab contribution，并把 active Canvas 打开为 `canvas://...` tab。
- `packages/app-web/src/features/workspace-runtime/model/types.ts` 使用 `WorkspaceRuntimeData` 聚合 session、frame runtime、extension runtime、VFS runtime surface、active canvas 等工作台数据。
- `packages/app-web/src/features/workspace-runtime/model/tab-types.ts` 用 `TabTypeDescriptor` / `TabInstance` 表达 WorkspacePanel 可打开模块。
- `RuntimeGateway` 已注册 `ExtensionRuntimeActionProvider`，extension action 调用时要求 Session context 携带 Project，并通过 Backend target 路由到本机 TS Extension Host。
- `RuntimeActionToolAdapter` 已存在，但规范明确它是 AgentTool 到 Gateway 的桥接基础件，不是默认注入策略。
- Session launch 阶段的 `assembled_tools` 当前没有从 Project extension projection 自动生成插件 action tool。
- `FrameLaunchIntent` / `ConstructionProjections` 已有 `extension_runtime` 字段，但 workflow frame construction 仍填 `None`，后续消费也未闭合。
- Canvas promote extension 已经把 Canvas files / entry / bindings / runtime bridge requirement 映射到 extension package draft，并通过 Project extension installation 进入 projection。

## 需求

- R1 定义 Workspace Module：以 AgentFrame/Session 的 Workspace 视角描述当前可协作模块，覆盖 UI、actions、channels、权限、状态和 trace provenance。
- R2 定义 Workspace Module Registry：作为 Agent 发现协作模块的唯一运行时投影入口，来源可以是 installed extension、Canvas instance、built-in workspace module、protocol adapter。
- R3 定义 Agent Workspace Module Tools：采用少量稳定工具，至少覆盖 list、describe、invoke、present/open；按需讨论 create/update 是否属于同一阶段。
- R4 定义 Extension 映射：Project enabled extension 的 runtime actions、protocol channels、workspace tabs 应被归并到 workspace module descriptor，而不是直接膨胀 Agent tool list。
- R5 定义 Canvas 映射：Canvas 是动态 workspace module instance；它可以被 Agent 创建、更新、展示，并在成熟后发布为 packaged extension，但运行协作协议保持一致。
- R6 定义调用约束：Agent 传 module_id 与 operation key，宿主解析 Project、Session、Backend、Workspace、AgentFrame、trace，不让 Agent 携带内部路由字段。
- R7 定义 schema 与审计：`workspace_module_describe` 提供 operation schema；`workspace_module_invoke` 在服务端做 schema 校验、权限裁决和 trace 记录。
- R8 定义术语分界：Workspace Module 是 Agent/前端协作层命名；Runtime Surface 是后端底层投影命名，例如 VFS surface、capability surface、MCP surface。
- R9 定义落地路线：本任务产出 parent/child 拆分建议；不直接进入实现，不修改运行代码。

## 验收标准

- [x] `design.md` 明确 Workspace Module / Workspace Module Registry / Workspace Module Tools / Workspace Module Instance 四个核心概念。
- [x] `design.md` 给出 Workspace Module 与 Runtime Surface 的术语边界。
- [x] `design.md` 给出 Extension、Canvas、Protocol Channel、Built-in Workspace Module 的映射表和数据流图。
- [x] `design.md` 解释为什么本任务选择稳定元工具路径，而不是把每个 extension action 注入为独立 Agent tool。
- [x] `design.md` 明确 Canvas 作为动态 workspace module instance 的生命周期：创建、展示、绑定、沉淀、发布。
- [x] `implement.md` 给出可拆分 child tasks、依赖顺序、候选修改面和验证命令。
- [x] 规划明确哪些现有事实源保持权威，哪些只是运行时 projection。
- [x] 用户过目并确认进入后续实现任务拆分（已确认：收口为 3 child，UI 落在 Child 3）。

## 范围边界

本任务（parent）只做概念收束、技术设计和后续任务拆分；运行代码、数据库迁移、前端界面和 API 契约实现由 3 个 child task 承接。由于项目仍处预研期，后续实现应以最正确的目标模型为准，不设计旧 API 或旧字段兼容路径。

Child 拆分（详见 `design.md` §12 与 `implement.md`）：

- Child 1 `06-08-workspace-module-read-contract`：读路径与单一 projection 契约。
- Child 2 `06-08-workspace-module-operate`：`workspace_module_invoke` / `workspace_module_present` 操作面。
- Child 3 `06-08-workspace-module-integration-ui`：集成 review + 项目层管理 UI + slug/文档改名收尾。

## 已决策（原开放问题收敛）

- D1（原 Q1）：首个 child = 只读路径。`workspace_module_list/describe` 与单一 projection 契约先行，零执行副作用，先验证 Agent 发现路径；invoke/present 收进 Child 2 独立验收。
- D2（原 Q2 + 调用分支）：Canvas 首轮只映射已存在 Canvas 为可发现/可展示/可调用 module，`workspace_module_invoke` 的 canvas 分支**包现有 canvas application service**，不另起 authoring 路径；Agent 主动 create/update authoring 留作 Child 2 可选尾段或后续任务。
- D3（原 Q3）：Protocol Channel 不独立成 module，`protocol_channels.methods` 作为其 provider extension module 的 operations 投影，与 `runtime_actions` 同构。descriptor DTO 因此只有一种 module 形态。
- D4（AgentFrame 锚点）：Workspace Module 可见性裁切在 AgentFrame **预留字段**，但解析走完整 Capability 能力通道（经 `CapabilityState` / `effective_capability_json`），不另开旁路；该契约在 Child 1 定死。
- D5（设置页）：项目层"Canvas + Extension 贡献的 WorkspaceModule 合并认知与管理"是同一份 canonical projection 的另一个消费端；**契约在 Child 1 定死**，UI 实现落在 Child 3。
- D6（死字段收口）：现有 `construction.rs` / `frame_construction/mod.rs` 中永远为 `None` 的 `extension_runtime` 字段，由 Child 1 接管为新 projection 或删除，不留半截路径。

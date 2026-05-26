# TS Extension Host 与插件 SDK 闭环设计

> 状态：planning
> 来源：2026-05-26 对话预研
> 关联背景：`.trellis/tasks/04-12-plugin-extension-api/`

## Goal

为 AgentDash 设计一套面向用户和项目的 TypeScript 插件开发闭环，让插件作者可以在独立仓库中使用 AgentDash SDK 开发、调试、打包插件，并让项目安装后获得对应的后端 runtime action、workspace 面板、命令、渲染器与可审计能力声明。

该任务聚焦插件体系规划，不直接进入实现。规划完成后应能拆出可独立验证的子任务，并为第一阶段 MVP 提供清晰边界。

## User Value

- 插件作者可以像开发 VS Code extension 一样，在独立环境里完成后端协议与前端面板的闭环开发。
- 项目用户安装插件后，无需重启云端主服务即可在当前项目获得新增命令、runtime action 与 workspace panel。
- 本机能力通过 `agentdash-local` 与 TS Extension Host 承载，既保持本机开发体验，也不把用户代码热插入 Rust 主后端或主前端进程。
- AgentDash 平台继续保留 Shared Library、Project installation、RuntimeGateway、session construction、WorkspacePanel 等现有权威链路，插件能力可审计、可升级、可投影。

## Confirmed Facts

- `agentdash-plugin-api` 当前是 Rust native host plugin SPI，适合 Auth、Connector、MountProvider、RoutineTriggerProvider、plugin embedded Shared Library assets 等启动期高权限能力。
- `extension_template` 与 `project_extension_installations` 已存在。Shared Library 契约规定 `ExtensionTemplate` 安装后只有 Project extension installation 才会进入运行时读取。
- `ExtensionTemplatePayload` 目前包含 `commands`、`flags`、`message_renderers`、`capability_directives`、`asset_refs`。
- session construction 已经从 project extension installations 聚合 `extension_runtime`，包括 installations、commands、flags、message_renderers。
- `/sessions/{id}/context` 当前返回 workspace / VFS / runtime surface / context snapshot / session capabilities，尚未把 `extension_runtime` 暴露给前端。
- 前端 `WorkspacePanel` 已有 `TabTypeDescriptor` 与 `tabTypeRegistry`，内置 canvas / vfs / terminal / context / inspector 都是注册式 tab type。
- Canvas runtime bridge 已经通过 `RuntimeGateway` 调用 session runtime actions；Runtime Gateway 规范要求消费端使用 `surface_for_actor` 与 `invoke`，actor/context 由宿主组装。
- `pnpm-workspace.yaml` 当前收纳 `packages/*`，适合新增 `@agentdash/extension-sdk`、`@agentdash/extension-ui`、`@agentdash/extension-dev` 等 TS package。
- 2026-05-26 讨论已明确：插件 UI 的默认目标是用户自定义前端 bundle。AgentDash 不应把插件 UI 限定为 schema-driven 表单；平台职责是提供 sandbox 容器、生命周期、加载/权限治理，以及让 UI 通过 SDK bridge 正确访问后端与本机 runtime 的信道。
- 2026-05-26 讨论已明确：首版 extension scope 采用 project-level。插件能力通常跟 Project 绑定，其它项目可通过 Marketplace / package 快速安装获得同类能力；user-level global extension 不进入 MVP。
- 2026-05-26 讨论已明确：插件 archive 的首版权威存储放在后端 / Project asset 侧，本机 local store 只承担 dev mode、下载缓存与运行解包缓存。
- 插件运行灵活度不应限制为单个纯 TS/JS 文件。插件可以依赖 npm 包，但正式安装包必须把运行所需依赖作为 bundle 或 package contents 自包含交付；AgentDash 不应在用户安装时对任意插件执行 `pnpm install` / `npm install`。

## Requirements

### R1. 插件稳定性分层

规划必须明确四层插件边界：

- **Native Host Plugin**：Rust 启动期高权限扩展，保留管理员安装/重启边界。
- **Runtime Extension Asset**：Project/User 可安装资产，进入 Shared Library 与 Project installation。
- **TS Extension Host**：由 `agentdash-local` 管理的本机 TypeScript 插件运行时，承载用户可开发 runtime actions、本机协议适配与 dev reload。
- **Frontend Extension Surface**：通过 manifest 贡献 workspace tabs、message renderers、panel renderers，并通过受控 bridge 调用 RuntimeGateway。

### R2. SDK 开发闭环

规划必须覆盖 SDK 三件套：

- `@agentdash/extension-sdk`：插件 activation、registration、runtime action、command、panel、renderer、permissions API。
- `@agentdash/extension-ui`：面板/webview 侧调用 API，包括 invoke action、open tab、read/write VFS、subscribe event、emit extension message。
- `@agentdash/extension-dev`：CLI 支持 init、dev、validate、pack、install。

### R3. Manifest 与安装契约

插件包必须产出可验证 manifest，至少表达：

- extension identity、display name、package version、asset version。
- commands、flags、message renderers。
- runtime actions 与 JSON schema / Zod-derived schema。
- workspace tabs 与 renderer declaration。
- permissions、local runtime requirements、asset refs。
- bundle refs 或 local dev refs。
- bundled dependency metadata、engine/runtime requirements、native dependency constraints。

安装后必须进入现有 Project extension installation 运行路径，而不是绕过 Shared Library / Project Asset 模型。

MVP 不应只支持 native plugin embedded extension。首版应支持外部开发者通过 SDK 打包出可安装 archive；first-party embedded seed 只用于内置示例、系统 Marketplace 预置或测试 fixture。

插件 archive 的正式安装事实源必须在后端可审计存储中，Project extension installation 引用该 artifact 的 digest / storage ref / source version。本机 `agentdash-local` 按需下载、校验 digest、解包运行。local path install 只作为开发模式，不作为团队共享或 Marketplace 安装的长期事实。

正式 archive 必须自包含生产运行依赖：推荐由 `extension-dev pack` 使用 bundler 产出 extension host bundle 与 panel bundle；允许 archive 内包含必要的 runtime files，但安装阶段不执行依赖安装脚本。包含 native addon、平台二进制、postinstall 下载逻辑的插件必须显式声明 runtime requirements，并在 MVP 中默认不作为通用路径。

### R4. TS Extension Host 运行模型

规划必须定义 `agentdash-local` 与 TS Extension Host 的职责：

- 发现项目启用的 TS extensions。
- 启动、停止、健康检查、reload 插件 worker。
- 接收插件 registration 并上报给云端 runtime surface。
- 执行 runtime action handler。
- 管理权限、cwd、环境变量、本机文件访问、HTTP 访问与外部进程调用边界。
- 将插件执行结果回传 RuntimeGateway trace 链路。

### R5. RuntimeGateway 协议闭环

插件 runtime action 必须成为 RuntimeGateway 可调用能力：

- 前端和 Canvas/panel 只发送 `action_key + input`。
- API route / host 组装 actor、context、trace、policy。
- 插件 action 通过 provider 或 proxy provider 进入 RuntimeGateway。
- action key、schema、权限声明进入 session runtime projection。

### R6. WorkspacePanel 插件贡献点

规划必须定义如何将 Project extension installation 的 workspace tab declaration 投影到前端：

- `/sessions/{id}/context` 或等价 current runtime projection DTO 暴露 `extension_runtime`。
- 前端从 projection 注册动态 `TabTypeDescriptor`。
- `workspaceTabStore` 的持久化布局能保存并恢复 plugin tab 的 `type_id + uri`。
- 插件 tab 首版主路径应支持 sandboxed webview / iframe renderer，用于加载用户自定义 UI bundle。
- schema-driven runtime panel 可作为开发诊断、低代码辅助或 fallback renderer，但不是插件 UI 能力上限。

### R6.1 UI SDK Bridge

插件 UI bundle 不能直接获得主前端 token、全局 store 或后端内部对象。它只能通过 `@agentdash/extension-ui` 暴露的 bridge 调用平台能力：

- invoke runtime action。
- open / activate workspace tab。
- read / write VFS。
- subscribe runtime or extension events。
- emit extension message or panel event。
- read scoped project/session/panel metadata。

### R7. Canvas 转插件

规划必须覆盖 “Promote Canvas to Extension”：

- Canvas files、entry、sandbox config、bindings、runtime bridge requirements 可以被打包成 extension template。
- 安装后生成 workspace tab，并复用 Canvas runtime preview 或其抽象版 renderer。
- Canvas 转插件应成为首个验证前后端插件闭环的示例之一。

### R8. 安全、权限与审计

插件体系必须有权限声明与审计模型：

- 插件声明需要的 HTTP/VFS/process/env/runtime permissions。
- 用户安装或启用时能看到权限摘要。
- host API 根据权限裁决，不把主前端 token、后端数据库连接或 relay secret 暴露给插件代码。
- runtime invocation 保留 trace、action key、extension identity 与 Project/session 关联。

### R9. 示例与验收样本

首批设计必须包含至少一个端到端样例：

- `gitlab-review` 或同类示例：注册一个 runtime action，贡献一个 workspace panel，panel 调用 action 展示结果。
- `canvas-promoted-extension` 示例：从 Canvas 打包成 extension template 并安装到 Project。

## Acceptance Criteria

- [ ] `prd.md`、`design.md`、`implement.md` 完整描述 TS Extension Host + SDK + frontend panel + RuntimeGateway + Project installation 的端到端方案。
- [ ] 设计明确 native Rust plugin 与 TS user extension 的职责边界，避免把两者混为一个安装模型。
- [ ] 设计明确 SDK 包结构、插件项目结构、manifest 生成/校验/打包/安装流程。
- [ ] 设计明确 `extension_template` / `project_extension_installations` 如何扩展以承载 workspace tabs、runtime actions、permissions 与 bundle refs。
- [ ] 设计明确 `agentdash-local` 与 TS Extension Host 的通信协议方向、生命周期、reload 和权限裁决。
- [ ] 设计明确 RuntimeGateway 如何代理插件 action，并保持 actor/context/trace 由宿主组装。
- [ ] 设计明确前端 WorkspacePanel 如何消费 `extension_runtime` 并注册动态 tab。
- [ ] 设计包含 Canvas 转插件的产品路径与技术映射。
- [ ] `implement.md` 给出可拆分执行阶段、主要修改文件、验证命令和回滚点。

## Scope Boundary

本任务输出架构规划与实施拆分。正式实现应拆成子任务逐步推进，优先打通一个最小闭环：

1. Project extension runtime projection 暴露到前端。
2. TS SDK manifest validate/pack。
3. local TS action host 最小运行时。
4. RuntimeGateway proxy provider。
5. WorkspacePanel dynamic tab + sandboxed webview bridge + 一个 sample extension。

远程 marketplace 分发、第三方插件签名生态、完整权限 UI、生产级 sandbox hardening 可以进入后续任务；但 webview/custom UI bridge 属于首个 MVP 主链路。

## Open Questions

- 是否需要把首个实现任务拆成父子任务树，还是先保持单个 planning task 等后续实现前再拆？

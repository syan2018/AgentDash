# Extension App 一体化 SDK 收束

## Goal

把当前 extension authoring 入口收束为一个对外 SDK 与一个 CLI，让用户和 AI 以“写一个 AgentDash Extension App”的方式完成开发、预览、打包和安装。

最终对外只保留：

- `@agentdash/extension`
- `agentdash-ext`
- `Extension App` 这个用户概念

现有 host SDK、panel bridge、dev toolchain 能力合并到同一个包内维护。用户默认只写 `agentdash.app.ts` 与普通前端代码，工具链负责生成 manifest、host entry、panel bridge glue 与 package artifact。

## Background

当前 extension 运行链路已经具备 packaged artifact、webview panel、TS host、runtime action、protocol channel、dev preview、pack/install 等基础能力。主要问题在 authoring interface：用户需要同时理解 `extension-sdk`、`extension-ui`、`extension-dev`、manifest、host/panel 分工，和“一把梭 app”式开发心智不一致。

本任务不重做后端 runtime/package 主链路；单 SDK、CLI、模板和示例先收束 authoring 入口，协议字段、runtime projection、Workspace Module operation 暴露和 `backendService` lifecycle manager 按设计接入既有后端执行链路。

具体集成规则、生成管线、capability 映射与既有 Web App 包装规则见同目录 [design.md](./design.md)。

面向后续手册的流程图草案见 [handbook-flow-draft.md](./handbook-flow-draft.md)。

后端、协议和工具链只读 review 摘要见 [subagent-review-summary.md](./subagent-review-summary.md)。

## Requirements

1. 新增并使用单一对外包 `@agentdash/extension`。
2. 将现有 extension host、panel bridge、dev/pack/install toolchain 能力合并进 `packages/extension`。
3. 新模板只依赖 `@agentdash/extension`，不再要求用户直接依赖多个 extension 包。
4. 默认 authoring 入口支持 `defineApp` 与常用 capability recipes，至少覆盖：
   - 远端 HTTP/API 调用
   - 本机命令
   - `backendService`：由当前本机 backend 自动拉起和访问的插件自带后端服务
   - workspace 文件读写
   - 自定义 channel 的高级逃生口
5. 默认 UI 侧入口支持 `useAgentDash`，用于调用 `agentdash.app.ts` 声明的能力。
6. `agentdash-ext dev / validate / pack / install` 继续形成闭环，并能消费新的 Extension App 项目结构。
7. 支持既有简单 Web App 的导出路径：收集静态构建产物，自动生成最小 extension 包；对依赖 `/api/**` 的 app，支持显式 fetch route 接到 `customChannel`、`backendService` 或远端 `httpProxy`。
8. 文档和示例改成单入口叙事：创建 Extension App、写 UI、加能力、预览、打包、安装。
9. 当前项目处于预研期，本任务按正确形态直接收束，不做旧包兼容层。
10. 协议层对 App 作者和 AI 编码助手暴露的主概念收束为 Capability；Agent 可调用能力作为 capability 的 exposure 标注生成到既有 Workspace Module operation 调用面。`fetch_routes`、`backend_services`、bundle kind、bridge method 只作为生成和运行细节进入实现文档。
11. 最终交付必须同时跑通两条一等工作流：从零开发原生 Extension App，以及把既有简单 Web App 包装为 Extension。

## Acceptance Criteria

- [ ] AC1: 仓库中存在 `packages/extension`，并提供 `@agentdash/extension` 包与 `agentdash-ext` CLI。
- [ ] AC2: 新示例 Extension App 只依赖 `@agentdash/extension`，可运行 dev preview、validate、pack。
- [ ] AC3: `defineApp` 能生成或驱动生成 extension manifest、host entry、panel client glue，且与 runtime surface 校验一致。
- [ ] AC4: `useAgentDash` 能在 panel 中调用至少一个远端 API capability、一个本机命令 capability、一个 workspace 文件 capability。
- [ ] AC5: 既有 Web App 静态产物可通过 CLI 包装成 `.agentdash-extension.tgz`，并通过后端 extension package archive 校验；若声明 `backendService`，运行时调用必须路由到当前本机 backend，而不是由云端直接访问 localhost。
- [ ] AC6: `local-hello` / `protocol-demo` 级别示例迁移到单 SDK 入口后仍可打包与运行。
- [ ] AC7: 文档不再把 `extension-sdk` / `extension-ui` / `extension-dev` 作为新手入口；新手路径只讲 `@agentdash/extension` 与 `agentdash-ext`。
- [ ] AC8: 删除旧的独立 extension 包形态或将其源码直接并入 `packages/extension`；仓库 package graph 不再要求 extension 示例直接依赖旧包名。
- [ ] AC9: Extension App 从 capability exposure 标注生成的 operation catalog 能投影到 Workspace Module `describe/invoke`，Agent 只能调用显式声明且具备 schema/权限摘要的 operation。
- [ ] AC10: `backendService` 的完整设计覆盖 service bundle、backend materialize/start/health/stop、fetch route、Agent operation 暴露与云端不直连 localhost 的约束。
- [ ] AC11: 用户手册、示例和 Agent 可见描述不把 `fetch_routes`、`backend_services`、bridge method 讲成新的顶层开发概念；它们只在协议/实现层作为 Dispatch 或 Artifact 细节出现。
- [ ] AC12: 手册和验证用例同时覆盖 `create/dev/pack/install` 原生开发路径与 `wrap-webapp/validate/pack/install` 快捷导入路径。

## Out of Scope

- 不新增第二套 extension package、runtime gateway、Workspace Module 或本机执行面。
- 不引入 Capability Pack / Agent 级能力包。
- 不做旧包对外兼容承诺。

## Implementation Notes

- 包内可以按 `app / react / host / panel / toolchain / cli` 组织源码，但文档不把这些目录包装成用户必须理解的概念。
- manifest 与 generated host/panel glue 应尽量作为生成物，避免用户和 AI 同时维护多份事实源。
- dev preview 应优先解释 Extension App 的能力、请求、权限和生成 surface，帮助闭环调试。

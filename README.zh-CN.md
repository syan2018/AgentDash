# AgentDash

[English](README.md) | 简体中文

![AgentDash runtime control plane](docs/assets/readme-runtime-map.svg)

AgentDash 是一个面向企业级协作与扩展的 Agent 工作空间平台。它把分散在聊天窗口、本机进程和临时日志里的 Agent 工作，收束成可治理、可扩展、可观察、可恢复的 Project 工作空间。

每一次 Agent 工作都拥有明确的归属：Project、AgentRun workspace、durable mailbox、runtime surface、本机或服务器执行边界、extension module，以及可以被检查、恢复、共享和自动化的事件证据。

## 核心想法

AI Agent 正在变成团队成员，但多数 Agent 工具仍然像临时终端：启动一个进程，流式输出一段聊天，然后希望日志足够说明发生了什么。只要进入团队场景，这种形态就会遇到共享项目、私有工作空间、服务器 runner、可复用能力、人类审查、自动化和可追踪执行的问题。

AgentDash 从另一个假设出发：

> Agent 工作应该是一种受管理的 workspace，拥有身份、权限、能力、运行上下文、产物和审计证据。

因此它不只是个人 Agent 实验工具，而是一个适合企业协作和平台扩展的 Agent 控制面。

## 特色能力

| 能力 | AgentDash 提供什么 |
| --- | --- |
| Project-centered collaboration | Project 锚定权限、settings、agent、asset、workspace binding、runner access 和共享上下文。 |
| AgentRun workspace | 每次运行拥有 chat、mailbox、lifecycle trace、VFS、terminal output、inspector panel、canvas 和 extension tab。 |
| Durable mailbox | 人类输入、companion 响应、routine 触发、hook 和系统命令进入同一个可恢复投递通道，而不是散落在临时 chat event。 |
| Cloud/local execution | 云端持有控制面事实；desktop、本机或服务器 runner 主动连接 Relay，在 workspace 附近执行。 |
| Runtime surfaces | VFS mount、MCP server、Skill、context frame、capability 和 workspace module 在 Agent 启动前完成投影。 |
| Extension platform | 打包后的 TypeScript Extension 可以贡献 runtime action、protocol channel、workspace panel、permission 和 Agent 可见 operation。 |
| Workspace modules | Extension、Canvas 和平台内置能力被统一成 Agent 可 list、describe、invoke、present 的模块。 |
| Evidence-first events | Connector 输出统一进入 Backbone event，用于推送、回放、UI 渲染、有界产物和审计。 |

## 核心体验

### 面向团队

- 创建 Project 作为协作与权限边界。
- 把 Project 共享给用户或用户组。
- 绑定 workspace 和 runner access。
- 配置 project agent、skill、MCP preset、VFS mount、routine、canvas、workflow 和 extension。
- 启动 AgentRun，并把对话、mailbox、运行证据、产物和 workspace 工具保存在同一个工作空间里。

### 面向 Agent 操作者

- 让 Agent 使用云端控制面上下文，同时把工具执行路由到被授权的本机或服务器。
- 用统一 Relay 边界承载 file、shell、MCP、VFS、extension 和 terminal 操作。
- 通过 mailbox receipt、source identity、claim token 和 scheduler outcome 恢复投递失败。
- 检查 Agent 实际看到的上下文、能力和 runtime surface。

### 面向平台建设者

- 把企业内部系统打包成 AgentDash Extension。
- 用 typed protocol channel 暴露业务协议，而不是堆一次性脚本。
- 把 Canvas 体验提升为可复用的 Workspace Module。
- 增加带结构化 permission 和 trace metadata 的 runtime operation。
- 用生成契约和统一事件协议保持前端、后端、本机 runtime 与 extension surface 对齐。

## 平台形态

```text
Project
  -> ProjectAgent / Story / Task / Routine / Workflow
  -> AgentRun workspace
  -> LifecycleRun + AgentFrame
  -> Runtime surface
       VFS mounts
       MCP servers
       Skills and memory
       Context frames
       Workspace modules
       Extension operations
  -> Connector / Runtime Gateway
  -> Cloud agent, function activity, local runner, server runner, or extension host
  -> Backbone events, artifacts, mailbox receipts, lifecycle evidence
```

关键边界很清楚：

- 云端后端拥有协作状态、权限、Lifecycle 事实、runtime surface、Session 事件、Extension 安装事实和部署发现信息。
- 本机与桌面 runner 拥有靠近机器的执行能力：文件系统、Shell、MCP、Terminal、第三方 Agent 和 Extension Host 调用。
- Relay 连接二者，不要求开发者机器开放入站访问。

## 扩展平台

AgentDash Extension 面向真实 workspace 集成，而不是装饰性的插件按钮。

一个 Extension package 可以提供：

- 面向 Agent 或 UI 调用的 runtime action；
- 可复用 provider API 的 protocol channel；
- workspace tab 和 panel；
- permission 与安装 metadata；
- 带 artifact digest 的 TypeScript host bundle；
- 通过 `@agentdash/extension-ui` 进行 panel 到 runtime 的 bridge 调用。

Workspace Module 再把 extension operation、canvas 和内置能力变成统一的 Agent-facing catalog。Agent 不需要知道某个能力来自 Extension、Canvas 还是平台内置，只需要查询可用模块、检查 schema、调用 operation 或展示 UI。

推荐入口：

- [Extension system](docs/extension-system.md)
- [Protocol demo extension](examples/extensions/protocol-demo/README.md)
- [Local hello extension](examples/extensions/local-hello/README.md)

## 企业协作

AgentDash 仍处于预发布阶段，但核心结构已经围绕企业协作需求设计：

- personal / enterprise 两种认证模式；
- Project 级用户与用户组授权；
- owner / editor / viewer 访问角色；
- system、user、project 三层 settings scope；
- Project-scoped runner registration；
- backend access 与 workspace routing；
- server-issued relay credential；
- durable AgentRun mailbox receipt；
- 有界 Backbone event 与 lifecycle VFS artifact；
- cloud image、migration、doctor、version 和 discovery endpoint。

目标不是把复杂性藏在 chat UI 后面，而是让 Agent 工作足够可治理，能够服务团队协作，同时保留本机、桌面、服务器和 Extension 驱动工作流的弹性。

## 快速启动

```bash
pnpm install
pnpm dev
```

`pnpm dev` 会构建 Rust debug binary，执行云端 migration，启动云端后端、本机 runtime 和 Web Dashboard。

| 服务 | 地址 |
| --- | --- |
| Cloud API | `http://127.0.0.1:3001` |
| Web Dashboard | `http://127.0.0.1:5380` |
| Relay WebSocket | `ws://127.0.0.1:3001/ws/backend` |

Rust 后端 binary 不支持热重载。修改 Rust 后需要停止旧 dev 进程再重新启动。

常用命令：

| 命令 | 用途 |
| --- | --- |
| `pnpm dev` | 启动默认 Web profile：云端后端、本机 runtime、Web Dashboard。 |
| `pnpm dev:desktop` | 启动带 Tauri 壳的 desktop profile。 |
| `pnpm dev:web:no-local` | 不启动本机 runtime。 |
| `pnpm run check` | 执行 contracts、后端检查/测试、前端检查/测试和 critical e2e。 |
| `pnpm run docker:cloud:build` | 构建云端部署镜像。 |

## 仓库地图

```text
crates/
  agentdash-api                         Cloud API、Relay endpoint、Web serving
  agentdash-domain                      Project、AgentRun、mailbox、permission、workflow facts
  agentdash-application-*               Runtime session、VFS、lifecycle、workflow、hooks、skills
  agentdash-workspace-module            来自 Extension 和 Canvas 的 Agent-facing modules
  agentdash-agent / agentdash-executor  Agent runtime 与 connector execution
  agentdash-relay / agentdash-local     Cloud/local protocol 与 runner
  agentdash-contracts                   Rust 到 TypeScript DTO 生成
  agentdash-agent-protocol              Backbone event protocol

packages/
  app-web                               Web Dashboard
  app-tauri                             Desktop shell frontend
  core / ui / views                     共享前端基础
  extension-sdk / extension-ui          Extension authoring 与 panel bridge
  extension-dev                         Extension dev、validate、pack、install 工具

examples/extensions/
  local-hello                           最小 Extension
  protocol-demo                         Runtime action + channel + panel 示例
```

## 延伸阅读

想进一步理解或扩展平台时，优先读这些文档：

- [项目总览](.trellis/spec/project-overview.md)
- [Extension system](docs/extension-system.md)
- [本机执行面](docs/local-execution-backend.md)
- [Backbone Protocol](.trellis/spec/cross-layer/backbone-protocol.md)
- [VFS 访问契约](.trellis/spec/backend/vfs/vfs-access.md)
- [AgentRun Mailbox](.trellis/spec/backend/session/agentrun-mailbox.md)
- [部署入口](deploy/README.md)

## 状态

AgentDash 处于活跃预研和产品开发阶段，还不是稳定公开产品。项目当前优先保证架构正确，而不是兼容早期假设。

## License

MIT

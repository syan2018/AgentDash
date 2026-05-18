# Plugin 动态安装与重启边界讨论

> 来源：2026-05-18 讨论，参考 `references/pi-mono`
> 关联任务：`04-12-plugin-extension-api`

## 背景

`pi-mono` 的扩展体验更接近本地 Agent Runtime 的热加载模型：用户可以把 TypeScript 扩展放到全局或项目目录，通过启动参数或 `/reload` 让当前 runtime 重新发现扩展。扩展可以注册命令、工具、事件 hook、UI 组件、provider、资源路径等能力，用户感知上接近“动态安装、即时生效”。

AgentDashboard 当前的 `AgentDashPlugin` 更偏宿主级 Rust SPI：插件由服务器启动时收集，注册 Auth、Connector、MountProvider、RoutineTriggerProvider、额外 skill 目录等高权限能力。以当前实现看，这类 native plugin 需要管理员部署并重启服务器后生效。

本讨论的核心问题是：AgentDashboard 是否只能依赖管理员重启整服，还是可以提供类似 `pi-mono` 的无感动态安装体验。

## 初步结论

AgentDashboard 不应直接照搬 `pi-mono` 的“动态加载任意 TypeScript 代码到主进程”模式。更合理的方向是将扩展拆为多层：

1. **Native Host Plugin**
   - 管理员安装，服务器重启生效。
   - 适用于 Auth、AgentConnector、MountProvider、RoutineTriggerProvider、宿主级服务注入等高权限能力。
   - 该层不追求普通用户无感热加载。

2. **Runtime Extension Asset**
   - 项目管理员或普通用户安装，无需重启。
   - 适用于 skill、prompt template、hook preset、Rhai rule、slash command、runtime flag、MCP preset、workflow/lifecycle template、routine template、capability directive、message renderer 描述等数据化能力。
   - 安装动作应写入数据库或项目级配置，由 session construction / capability resolver 动态读取。

3. **External Extension Service**
   - 外部服务动态注册，无需重启 AgentDashboard 主服务。
   - 适用于 MCP server、HTTP extension service、webhook provider、local backend sidecar、企业内网服务等。
   - AgentDashboard 保存 endpoint、auth、schema、capability 声明，运行时通过 MCP / HTTP / relay 调用。

4. **Frontend Extension Surface**
   - 短期优先采用 schema-driven UI，不直接热加载任意 React 代码。
   - 适用于 custom message card、tool card、form、status widget、ContextFrame section 等可由通用组件渲染的扩展界面。
   - 中长期如需动态 UI bundle，应考虑 iframe、signed bundle、CSP、权限声明和加载失败降级等安全边界。

因此，更准确的产品目标不是“所有插件都无需重启”，而是：

> Native 宿主扩展需要管理员重启；用户可感知的大部分能力扩展应通过动态资产或外部服务实现，无需重启整服。

## 与 `pi-mono` 的关键差异

`pi-mono` 的动态安装体验建立在以下前提上：

- 本地单用户 Agent runtime。
- TypeScript 扩展通过 `jiti` 动态加载。
- 扩展默认拥有较高权限，可直接读写文件、注册 tool、改 UI、拦截事件。
- `/reload` 本质是重建当前 runtime 的 extension registry。

AgentDashboard 的约束不同：

- 服务端是 Rust 长进程，当前 plugin 以 trait object / crate 装配为主。
- Cloud / Local / Frontend 三段式架构中，业务状态、会话、工具执行、前端展示不在同一个 runtime。
- 多用户、多项目共享同一服务端，不能轻易让普通用户安装任意代码并热插入主进程。
- Hook、CapabilityState、VFS、MCP、ContextFrame 已经形成平台管线，扩展能力应进入这些可审计、可回放、可热更新的管线。

## 推荐分层

### 1. Native Host Plugin

这层保持启动期注册：

- `AuthProvider`
- `AgentConnector`
- `MountProvider`
- `RoutineTriggerProvider`
- 需要访问宿主内部 repository / service 的 provider
- 需要持有后台任务或进程级资源的能力

这类能力涉及权限、生命周期和资源清理，不建议为了“无感安装”引入 Rust 动态库热加载。管理员部署和重启是可接受的治理边界。

### 2. Runtime Extension Asset

这层应作为 `04-12-plugin-extension-api` 的重点演进方向。扩展不必总是 Rust 插件，也可以是数据库中的可安装资产。

候选资产：

- Slash command
- Runtime flag
- Extension message type
- Hook preset / Rhai rule
- Skill / prompt template
- MCP preset
- Workflow / lifecycle template
- Routine template
- Capability directive
- VFS mount 配置
- Message renderer schema

这层的安装流程应支持：

1. 用户在 UI / marketplace 中安装 extension asset。
2. 资产写入用户、项目或 workspace scope。
3. 新 session construction 自动读取。
4. 运行中 session 可通过 capability transition / context frame 接收变化。
5. 前端通过 ContextFrame 或 event stream 展示新增能力。

### 3. External Extension Service

对工具扩展来说，MCP 应作为第一主路径。用户安装扩展时可以注册一个 MCP preset 或 relay MCP server，而不是要求主服务加载插件代码。

推荐路径：

- Extension manifest 声明 MCP server。
- 安装后写入 project/user scope 的 MCP preset。
- CapabilityResolver 注入 `mcp:<key>`。
- Runtime tool discovery 动态发现工具 schema。
- PiAgent / session runtime 通过现有 MCP 或 relay MCP 路径调用。

这样可以达到“安装后当前项目立刻多出工具”的体验，同时不扩大主服务代码执行面。

### 4. Frontend Extension Surface

短期不建议开放任意前端代码热插入。可以先支持 schema-driven renderer：

- `custom_type` 对应默认 JSON / Markdown / form / status card 渲染。
- Tool result 可按 `details.kind` 或 `custom_type` 选择内置 renderer。
- Message renderer 先由 first-party 前端注册，第三方 manifest 只声明展示 schema。

后续若需要动态 UI bundle，应单独设计安全模型。

## ExtensionManifest 草案

动态资产可收敛为一个声明式 manifest：

```json
{
  "id": "gitlab-review",
  "display_name": "GitLab Review",
  "version": "0.1.0",
  "scope": ["project", "user"],
  "commands": [
    {
      "name": "gitlab-review:prepare",
      "description": "准备 GitLab review 上下文",
      "handler": {
        "kind": "inject_message",
        "content": "请基于当前 MR 准备 review。"
      }
    }
  ],
  "flags": [
    {
      "name": "gitlab-review.verbose",
      "type": "bool",
      "default": false,
      "description": "输出更详细的 review 诊断"
    }
  ],
  "hook_presets": [],
  "skills": [],
  "mcp_presets": [],
  "capability_directives": [],
  "message_renderers": []
}
```

该 manifest 是讨论草案，不代表最终数据库结构。重点是把用户可动态安装的部分数据化，避免把所有扩展都绑定到 native plugin 重启模型。

## 与当前 PRD 的关系

现有 PRD 的三项能力仍然有效：

- `registerCommand`
- `registerFlag`
- `CustomMessage<T>`

但需要补充一个设计方向：

> 这些能力不应只作为 native plugin trait 暴露，也应支持 runtime asset / manifest 形式，从而实现无需重启的项目级或用户级动态安装。

换句话说，`AgentDashPlugin` 可以继续作为管理员级宿主扩展 API；`Plugin Extension API` 需要额外定义用户可安装的 runtime extension API。

## 建议路线

### 阶段 1：补齐动态资产最小闭环

- 定义 runtime extension asset 的存储和 scope。
- 支持 slash command asset。
- 支持 runtime flag asset。
- 支持 extension message 持久化和默认前端渲染。
- 在 session construction 中读取已安装 extension assets。

### 阶段 2：接入 Hook 和 Capability

- Hook Rhai ctx 支持读取 extension flags。
- Command handler 支持触发 hook 或注入 ContextFrame。
- Extension asset 可声明 capability directives。
- 运行中 session 可收到 capability/context delta。

### 阶段 3：MCP 作为动态工具扩展主路径

- Extension manifest 支持声明 MCP preset。
- 安装后无需重启即可出现在 project MCP 候选中。
- CapabilityResolver 支持按 extension asset 注入 `mcp:<key>`。
- Runtime tool discovery 动态刷新工具 schema。

### 阶段 4：前端渲染扩展

- 先提供 schema-driven message/tool renderer。
- 再评估 sandboxed UI bundle。

## 风险与边界

- Native plugin 热加载暂不作为优先目标。
- 用户动态安装不应获得任意服务器代码执行能力。
- Extension asset 必须有 scope、来源、版本、启用状态和审计记录。
- 运行中 session 热更新需要清晰展示变更，避免 Agent 无感获得高风险工具。
- MCP / external service 需要权限声明和调用审计。

## 设计原则

1. 用户感知的扩展尽量动态化。
2. 高权限宿主扩展保持管理员治理。
3. 工具扩展优先外部服务化，MCP 是首选协议。
4. 扩展能力进入 CapabilityState / Hook / ContextFrame 管线，保证可审计、可回放、可解释。
5. 前端扩展先数据化渲染，再考虑代码化 UI。

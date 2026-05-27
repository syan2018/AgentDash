# 插件系统完整能力对齐

## Goal

将当前以 `local-hello` / `ctx.api.local.getProfile()` 为主的最小插件闭环，对齐到原始规划中的完整插件系统：插件作者可以在独立 TS 插件项目里提供自己的 extension host bundle、runtime actions、业务协议适配、可复用协议信道与 workspace panel；AgentDash 负责 Project 安装、包校验、host lifecycle、受控内嵌能力 facade、跨插件/Canvas 信道注册与调用、必要的运行边界、RuntimeGateway relay、前端 webview bridge 与审计。

本任务的完成态不是再补一个单点 Host API，而是让“用户自写 TS host/action/protocol/channel”成为默认开发模型；`getProfile` 只作为平台内建能力示例存在。

## Confirmed Facts

- 原始任务 `05-26-ts-extension-host-sdk` 的目标是用户可开发的 TypeScript 插件闭环，而不是 Rust native plugin 或硬编码 demo API。
- 当前 packaged extension 安装链路已经可用：`extension-dev pack` 生成 archive，Project artifact/install 保存事实源，WorkspacePanel 能加载 webview bundle，panel 能通过 bridge 调 runtime action。
- 当前 TS Extension Host 已能加载插件 bundle、收集 contributions、执行 action handler，并通过 stdio JSON line 向 `agentdash-local` 请求 Host API。
- 当前 Host API facade 只有 `runtime.invoke` 与 `local.get_profile` 两条实际能力，`api.http` / `api.vfs` / `api.env` / `api.process` / panel VFS bridge 尚未形成完整实现。
- `getProfile` 的 username 来源在 `agentdash-local`，这是合理的 built-in host capability；问题是示例和 SDK 暴露面过窄，不能代表完整插件能力。
- 项目当前处于预研期，规划与实现可以朝正确模型收敛，不需要保留旧字段、旧 API 或兼容路径；如涉及数据库字段调整，应正常补 migration。
- 用户已确认采用推荐边界：平台管理 per-extension worker/process，插件提供自己的 `extension_host` bundle 和协议模块；插件不自行接管 AgentDash 控制面进程。
- 插件协议信道需要独立注册，其他插件可以依赖前置插件提供的信道完成访问；Canvas 后续也应能通过统一 bridge 使用插件提供的 API 信道。
- 插件闭合开发时需要高效调用自己维护的 host/channel，不应要求作者反复显式写自己的插件名；SDK 需要提供 self-scope 与 dependency alias 糖，底层仍保留 canonical channel key 用于审计。
- process/shell 能力按工具项目定位处理，短期不做高安全性权限设计；首版应提供低摩擦通用 shell/process 调用，同时保留 cwd、timeout、输出上限和 trace 这些工程护栏。
- 过度设计的权限门禁可以清理；权限声明应服务安装摘要、依赖解析、可用性诊断和审计，不把可信本机工具做成未信任插件市场。
- 用户已确认新增独立 `protocol-demo` 示例，`local-hello` 保持最小 Host API 示例定位。

## Requirements

- 插件作者必须能提供自己的 TS extension host entry。
  - 插件项目以 `src/extension.ts` 或 manifest 指定的 host entry 为 author-owned 代码入口。
  - 插件包打包后必须包含 self-contained `extension_host` bundle；安装端不执行 `npm install` / `pnpm install` / package lifecycle scripts。
  - 插件作者通过 `ctx.runtime.registerAction` 注册业务 action，通过普通 TS 模块组织自己的协议 adapter、client、parser、state helper 和业务逻辑。
  - 新增业务协议不需要修改 `runner.rs`、`permissions.rs` 或 Rust host API match 分支，除非它需要一个新的平台级 capability。
- 插件协议/API 信道必须是一等贡献。
  - 插件可以注册 Project/session scoped protocol channel，声明 channel key、methods、JSON schema、版本、权限与可用性。
  - 其他插件可以在 manifest 中声明依赖某个插件或某个 channel，并在自己的 action handler 中通过 SDK 调用该 channel。
  - Canvas runtime bridge 后续可以作为 consumer 调用已授权的 extension channel；Canvas 不需要为具体插件写硬编码 adapter。
  - 信道调用必须经过平台 discovery、admission、routing 和 trace；禁止 consumer 直接拿到 provider 插件的 JS object、内部 token 或本机连接细节。
  - Provider 插件未安装、未启用、版本不满足、backend 离线或权限不足时，consumer 和 Canvas 都必须得到可诊断错误。
- 插件协议信道需要提供开发体验糖。
  - Provider 插件注册未限定的本地 channel key 时，SDK 自动按当前 extension scope 生成 canonical key；插件自身调用该 channel 时可以使用 self-scope shortcut，不必写自己的插件名。
  - Consumer 插件通过 dependency alias 调用前置插件信道；业务代码使用 alias，runtime trace/projection 记录 canonical provider extension/channel。
  - Canvas bridge 使用同一 alias/binding 思路，Canvas 代码面向绑定名调用插件信道，不直接硬编码 provider extension key。
  - sugar 只影响 authoring ergonomics，不改变权限、依赖声明、runtime routing 和审计模型。
- 平台内嵌能力必须作为受控 Host API facade 暴露。
  - `api.local.getProfile()` 保留为 built-in local profile capability。
  - 补齐原规划中的首批受控 capability：HTTP、workspace/VFS file access、env secret/read、process/shell execution、runtime invoke。
  - 每类 capability 都必须有清晰 contract、错误语义、trace metadata 与测试；manifest declaration 和 action-level permission 只在确实服务安装摘要、依赖解析、诊断或审计时保留。
  - 已有过度门禁应在本任务中清理，不保留“声明了但对可信工具没有实际产品价值”的重复 deny path。
  - Host API registry/dispatcher 必须是可扩展结构，新增 built-in capability 时进入统一注册、参数校验、必要边界检查和审计路径。
- process/shell capability 首版采用粗粒度工具模型。
  - SDK 提供通用 shell/process 调用，不要求每个命令都先配置 manifest command allowlist。
  - 权限以 extension 顶层 `process` capability + action-level `process.execute` 表达。
  - Host 侧保留 workspace cwd 绑定、timeout、stdout/stderr 输出上限、exit code capture 和 trace metadata。
  - 不把 process capability 设计成高安全隔离边界；当前项目定位是本机可信工具，不是未信任插件市场。
- TS Extension Host runtime 需要从硬编码 demo runner 收口为可维护的 runtime layer。
  - runner 的 JS/TS runtime 源码应成为独立维护单元，再由构建或嵌入机制交给 Rust 使用。
  - runner 负责加载插件 host bundle、创建 `ExtensionContext`、维护 action invocation context、转发 Host API request、规范化错误和日志。
  - Rust `agentdash-local` 负责 supervisor、cache、workspace roots、backend/project/session activation、必要运行边界 enforcement 和本机事实源。
  - 执行模型继续按 trusted local extension 表达；Host API facade 的目的在于产品权限、协议稳定性与审计，不把当前 Node runner 宣称成 untrusted sandbox。
- SDK、manifest、domain contract、RuntimeGateway 与前端 bridge 必须对齐同一套 extension capability 模型。
  - `packages/extension-sdk` 暴露 typed authoring API，避免插件作者直接拼 host api method string。
  - SDK 需要同时支持 provider 侧 `registerChannel` 与 consumer 侧 `invokeChannel`，并让 channel invocation 继承 Project/session/backend/trace context。
  - `packages/extension-dev` validate/pack 检查 manifest channel/dependency、保留下来的 capability/permission、bundle refs、runtime requirements 与自包含依赖。
  - `ExtensionTemplatePayload` / Project installation / runtime projection 继续作为平台事实源，新增权限类型、channel declaration 和 dependency declaration 需要进入后端 domain validation 与 generated frontend contracts。
  - WorkspacePanel webview bridge 必须支持文档承诺的 panel-side 能力；不能只在 `extension-ui` 类型里声明但宿主不处理。
  - RuntimeGateway extension provider 必须在 action/channel invocation admission 阶段校验 Project installation、provider/consumer dependency、target backend、package artifact 与必要的可用性边界。
- 示例必须覆盖两种插件开发心智。
  - 保留或更新 `local-hello.profile`，展示 built-in Host API、local profile 权限和端到端调用链。
  - 新增独立 `examples/extensions/protocol-demo`，展示纯 TS action、业务协议 adapter、protocol channel provider/consumer、self-channel shortcut、dependency alias、Canvas-like panel consumer 与通用 process/shell。
  - `protocol-demo` panel 需要展示调用多个 action/channel 的真实流程，并说明每个 action/channel 使用了哪些 capability 或 runtime binding。
- 文档必须提供开发和使用视角。
  - 在 `docs/` 下新增插件系统开发文档，面向插件作者和平台维护者说明 mental model、项目结构、manifest、Host API、打包、安装、调试和示例。
  - 文档要明确 `getProfile` 的来源和定位：它是 built-in host capability，不是浏览器/前端原生信道，也不是插件系统的能力上限。
  - 文档必须包含“用户自写 TS host/protocol”的用例说明，展示从 `src/extension.ts`、协议 adapter、manifest permission 到 panel 调用的完整路径。
  - 文档必须包含“插件提供 API 信道，其他插件或 Canvas 消费信道”的用例说明，展示 provider/consumer manifest、权限、调用链与错误状态。

## Acceptance Criteria

- [ ] `@agentdash/extension-sdk` 暴露完整 TS authoring surface：runtime action、workspace panel、built-in Host API facade、typed permission declarations 和 clear no-host fallback 行为。
- [ ] `agentdash-local` 的 TS Extension Host 不再以 `local.get_profile` demo path 为结构中心；Host API 通过 registry/dispatcher 统一处理参数、权限、执行和错误。
- [ ] 插件包可以携带 author-owned TS host bundle 与任意 TS 协议模块；新增普通业务协议 action 不需要改 Rust。
- [ ] 插件可以注册独立 protocol channel；其他插件和 Canvas 可以通过声明依赖、通过统一 SDK/bridge 调用这些信道。
- [ ] SDK 支持 self-channel shortcut 与 dependency alias；插件调用自己的 channel 不需要写自己的插件名，跨插件/Canvas 调用可以使用声明过的 alias。
- [ ] HTTP、workspace/VFS、env、process、runtime invoke、local profile 至少各有 contract、错误语义、trace metadata 和 focused tests；保留下来的权限声明必须有明确产品用途。
- [ ] 清理现有过度设计的权限门禁，不保留只增加摩擦而不提供真实诊断、依赖解析或审计价值的 deny path。
- [ ] process/shell capability 提供低摩擦通用执行接口，并记录 cwd、timeout、输出限制、exit code 和 trace；不要求 command allowlist 才能使用。
- [ ] `extension-dev validate/pack` 能校验新增 manifest permission、runtime requirements、bundle refs 与自包含包约束。
- [ ] RuntimeGateway extension action/channel admission 与 local host enforcement 使用同一运行边界语义；拒绝结果能定位 provider extension、consumer extension/Canvas、action/channel key、capability 或 binding 原因。
- [ ] WorkspacePanel webview bridge 与 `@agentdash/extension-ui` 类型一致；已声明的 VFS/metadata/event/runtime 能力要么真实接通，要么在文档和 SDK 中不再作为可用 API 暴露。
- [ ] 示例插件覆盖 built-in Host API、独立 `protocol-demo`、纯 TS action、用户自写协议 adapter、protocol channel provider/consumer、webview panel 调用、pack/install/session 试用全链路。
- [ ] `docs/extension-system.md` 或等价文档新增并包含完整用例说明；`examples/extensions/*/README.md` 与主文档互相指向。
- [ ] 相关 unit/integration tests 覆盖 SDK context、manifest validation、domain permission evaluator、local host API/channel enforcement、RuntimeGateway admission、panel/Canvas bridge handler、example extension pack/test。

## Out Of Scope

- 不把 Rust native plugin SPI 替换为 TS extension；两者仍是不同扩展线。
- 不在本任务内承诺第三方不可信代码的 OS 级 sandbox。当前目标是 trusted local extension + permissioned product facade；若要 untrusted sandbox，应另开安全隔离任务。
- 不引入安装时运行包管理器或 lifecycle scripts；插件包必须自包含。

## Open Questions

- 无。

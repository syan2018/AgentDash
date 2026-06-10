# Design - Canvas Agent Tools Converge Into workspace_module

## Assessment

当前状态是“投影已合并，工具面未合并”。

`workspace_module` 已经把 project canvas 聚合为 `canvas:{mount_id}` module，并且前端 Project Settings 能展示这个统一认知视图。但 Canvas module 的 `operations` 为空，Agent 无法通过 `workspace_module_invoke` 完成 Canvas 创建、接入或绑定；`workspace_module_present` 也没有承接 Canvas session 暴露语义。与此同时，`canvas` well-known capability 仍独立暴露四个工具：

- `canvases_list`
- `canvas_start`
- `bind_canvas_data`
- `present_canvas`

这个状态导致三类分裂：

1. **能力主入口分裂**：Agent 既可能先用 `workspace_module_list` 看见 Canvas，又必须回到 `canvas_start` / `present_canvas` 执行 Canvas 行为。
2. **session 暴露语义分裂**：`present_canvas` 会追加 Canvas VFS mount 并刷新 capability state；`workspace_module_present` 只发 workspace module 展示事件。
3. **前端 URI 分裂**：Canvas tab 以 `canvas://{mount_id}` 为 renderer URI；workspace module projection 当前给 Canvas UI entry 的 `uri_scheme` 是 VFS mount id（例如 `cvs-demo`），不是可直接打开的 tab URI。

因此，妥善合并不是删除一个文件，而是把 Canvas 的 host-owned operation 接入 workspace module operation dispatch，并把 capability catalog 与默认注入面一起切过去。

## Target Model

### Agent-facing Tool Surface

Agent 面向平台只看到 workspace module 工具：

- `workspace_module_list`
- `workspace_module_describe`
- `workspace_module_invoke`
- `workspace_module_present`

Canvas domain service、Canvas repository、Canvas VFS provider、Canvas panel 和 HTTP management API 保持内部边界。它们仍是实现 Canvas 资产管理的事实源，但不再作为 Agent tool capability 的并列入口。

### Module Shape

引入一个 host-owned Canvas module 作为“创建/入口”能力：

```text
builtin:canvas
  operations:
    canvas.create_or_attach
    canvas.list
```

已有 Canvas 资产继续作为实例 module：

```text
canvas:{mount_id}
  ui_entries:
    default -> presentation_uri canvas://{mount_id}
  operations:
    canvas.bind_data
```

如实现时希望更少 module，也可以让 `builtin:canvas` 同时承担 `bind_data` 并要求 input 带 `canvas_id`。评估推荐保留实例 module，因为 ProjectAgent 的 `visible_workspace_module_refs` 已经以 `canvas:{mount_id}` 表达可见性，实例 operation 和 UI entry 能自然继承该裁切。

### Canvas As A Built-In Workspace Module

评估倾向是：Canvas 创建、接入和绑定统一走 `workspace_module_invoke`，Canvas 展示统一走 `workspace_module_present`。对 Agent 来说，Canvas 系统应表现为一个平台内嵌的 workspace module family：

- `builtin:canvas` 是入口/工厂 module，负责 project 级 Canvas 列表、创建或接入。
- `canvas:{mount_id}` 是实例 module，负责该 Canvas 的绑定等实例级 operation，并提供 Canvas UI entry 供 `workspace_module_present` 展示。
- Canvas 源码编辑仍通过 VFS mount 完成；当 `canvas.create_or_attach` 或 `workspace_module_present(canvas:{mount_id})` 把 mount 暴露给 session 后，Agent 再按 `canvas-system` skill 编辑 `cvs-<mount_id>://...` 文件。

这等价于把“Canvas Agent 操作面”做成一个完整内嵌 module，但不是把 Canvas domain 合并成一个大而混浊的模块。内部仍保持清晰分层：

- workspace module 负责 discovery、operation schema、invoke routing 和 presentation facade。
- Canvas application use case 负责创建、绑定、present/session exposure、runtime snapshot 等业务语义。
- Canvas VFS provider 负责 `cvs-*://` 文件访问。
- Canvas panel/runtime 负责前端渲染和 iframe bridge。

这样做的原因是 Agent 只需要学习一套“module 操作协议”，而平台内部仍保留 Canvas 资产管理、VFS 和 UI runtime 的专门边界。未来如果还有其它复杂内嵌能力，也可以按同样模式进入 `workspace_module`，而不是为每个能力新增一簇顶层 Agent tools。

### Operation Dispatch

现有 `WorkspaceModuleOperationDispatch::Canvas { canvas_action }` 语义偏向 Canvas runtime action。老 Canvas 工具的实际行为是 host application operation，因此建议新增或重命名为 host-owned 分支：

```rust
HostCanvas { action: WorkspaceModuleCanvasHostAction }
```

`WorkspaceModuleCanvasHostAction` 至少覆盖：

- `List`
- `CreateOrAttach`
- `BindData`
- `Present`

`workspace_module_invoke` 根据 dispatch 进入 Canvas application use case。Canvas use case 需要从现有 AgentTool 包装中抽出，避免在 workspace module 工具里复制 repository / VFS / eventing 逻辑。

### Present Data Flow

Canvas presentation 必须由 `workspace_module_present` 复用现有 session 暴露逻辑：

```text
workspace_module_present(module_id=canvas:{mount_id}, view_key=...)
  -> load canvas by project + mount_id
  -> append canvas mount to SharedRuntimeVfs
  -> append visible canvas mount to AgentFrame / session capability service
  -> apply live VFS capability state
  -> emit workspace_module_presented
       presentation_uri = canvas://{mount_id}
       module_id = canvas:{mount_id}
       renderer_kind = canvas
```

前端只按 `presentation_uri` 打开 WorkspacePanel tab。Canvas panel 继续读取 Canvas runtime snapshot 和 VFS surface。

### Capability Catalog

`workspace_module` 应成为 Canvas Agent 操作的能力 key。实施时需要同步更新：

- `ToolCluster` / `CAP_*` 常量和映射
- `platform_tool_descriptors()`
- `CapabilityState::all()`
- `session::plan::conditional_flow_tools`
- tool provider 注入逻辑
- capability notification 文案
- frontend capability picker 类型和文案
- specs 中的 well-known capability matrix

`canvas` key 可以被删除，或仅作为内部历史迁移输入处理。由于项目未上线，推荐删除普通 well-known 暴露并通过 migration 处理已有配置。

### Agent Skill Guidance

需要配套一个轻量 `workspace-module-system` skill。

原因不是工具 schema 不够，而是 workspace module 的正确使用流程具有“协议性”：Agent 应先 `workspace_module_list` 找 module，再 `workspace_module_describe` 读取 UI entries / operations / schema，然后根据 operation dispatch 使用 `workspace_module_invoke`，或根据 UI entry 使用 `workspace_module_present`。Canvas 硬切后，Agent 还需要知道不再直接寻找 `canvas_start` / `present_canvas`，而是通过 `builtin:canvas` / `canvas:{mount_id}` operations 完成创建和绑定，通过 Canvas UI entry 完成展示。

这个 skill 应保持薄层：

- 只写 Agent 操作协议，不写 workspace module 聚合层内部实现。
- 说明 `module_id` 形态：`builtin:{key}` / `ext:{extension_key}` / `canvas:{mount_id}`。
- 说明推荐流程：`list -> describe -> invoke/present`。
- 说明 Canvas 特例：创建/接入走 host Canvas module；Canvas mount 出现后，编辑 Canvas 源码再加载 `canvas-system`。
- 说明 Extension 特例：operation input/output schema 以 describe 结果为准，provider/host 是最终语义校验者。

现有 `canvas-system` 不适合作为唯一指南，因为它通常随 Canvas mount 暴露，解决的是“如何编辑/运行 Canvas 资产”。在 Canvas 创建前，Agent 需要一个 session 级 bootstrap 指南告诉它如何通过 workspace module 进入 Canvas 能力。因此推荐新增 `workspace-module-system` 作为项目级内嵌 SkillAsset，类似 `companion-system` 通过 lifecycle VFS projection 注入；`canvas-system` 保留为 Canvas mount 内的作者指南，并在核心流程中改为引用 workspace module 入口。

### Migration

ProjectAgent config 存在 `project_agents.config` JSON。需要新增 forward migration：

- 将 `config.capability_directives` 中能力级 `{"add":"canvas"}` 改为 `{"add":"workspace_module"}`。
- 将 `{"remove":"canvas"}` 改为 `{"remove":"workspace_module"}`。
- 将工具级 Canvas path 映射为 workspace module 工具策略时要谨慎：老工具与新工具不是一一等价。推荐能力级转换，工具级 include/exclude 在实现前重新评估是否清理或转成最接近的 `workspace_module::<tool>` path。

Canvas 资产表、文件表、绑定表不迁移；它们仍是 Canvas 业务数据源。

## Trade-Offs

### Why Hard Cut

本项目处于预研期，保留两套等价 Agent-facing 工具会继续让模型选择不稳定，并迫使后续 capability、文案、测试都维护双轨。硬切能让 `workspace_module` 的定位真实成立，也让 ProjectAgent 的 module allowlist 与实际可调用面一致。

### Why Keep Canvas Internals

Canvas 是资产和 renderer，不只是工具。把 Agent-facing 工具收束到 workspace module，并不意味着把 Canvas domain、VFS provider 或 panel 合并进 workspace module 包。workspace module 是 projection / operation facade，Canvas domain 仍拥有数据、runtime snapshot、VFS materialization 和 promote-extension 等业务。

### Why HostCanvas Dispatch

Canvas create/bind 需要访问 repository 等 application service，不属于 extension runtime action。Canvas presentation 虽然通过 `workspace_module_present` 触发，也同样需要访问 VFS、session capability service 和 eventing。用 host-owned dispatch / present delegate 可以让路由表达真实 ownership，并避免把业务 use case 错建模成 iframe action。

### Why Skill Plus Tool Schema

工具 schema 能告诉模型“参数是什么”，但很难稳定表达跨工具顺序、何时需要 describe、如何从 module visibility 推导可调用面、以及 Canvas mount 出现前后应使用不同指南。`workspace-module-system` skill 只承载这层操作协议，避免把复杂使用规则塞进每个工具 description，降低描述膨胀和模型误用概率。

## Risks

- `workspace_module_present` 不能退化成单纯 UI 事件。对 Canvas renderer，它必须先执行 Canvas session exposure，再发 `workspace_module_presented`；对 extension webview/panel，则保持轻量打开 UI entry。
- 工具级 capability policy 若已有 `canvas::present_canvas` 这类路径，无法机械等价迁移到单个 workspace module operation。实现前需要 grep/测试本地 seed 和 fixtures，决定清理还是映射。
- 前端同时存在 `canvas_presented` 与 `workspace_module_presented` 事件处理。硬切后应移除或降级旧事件路径，避免继续让 `activeCanvasId` 成为隐藏事实源。
- 新 skill 若写得过厚，会变成第二份架构文档并快速漂移。应把它限制在 Agent 操作协议，详细 contract 仍由 `workspace_module_describe` 和 spec 提供。

## Planning Status

评估建议进入实现，但在 `task.py start` 前需要 review 本设计，确认采用硬切策略。

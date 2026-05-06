# 迭代 Workflow/PhaseNode 驱动的运行时表面与通知系统

## 目标

让 Workflow / Lifecycle step 变化能够可靠驱动 session 的能力模型更新，并让 PhaseNode 成为一等执行模式：同一个 Agent session 可以随 step 切换 workflow contract、工具能力、MCP、VFS/mount 装载和上下文约束。Agent、UI、审计历史都应能一致观察这些变化。当前 PhaseNode 相关逻辑已经有雏形，但顶层 Capability、tool capability key、FlowCapabilities、MCP 工具表、VFS/mount、通知持久化之间还没有形成一个单一权威模型和投影结果，因此本任务用于追踪后续分阶段收敛。

## 已知背景

* `CapabilityResolver` 已经是工具能力计算的核心纯函数，输入包含 owner context、agent declared capabilities、workflow capability directives、MCP presets / inline servers。
* `StepActivation` 已经尝试收口 step 激活时的能力、MCP、kickoff prompt、lifecycle mount 计算，但它还没有表达任意 step-specific mount overlay 的完整配置模型。
* PhaseNode 热更新路径目前由 `LifecycleOrchestrator::apply_activated_phase_nodes` 触发，最终走 `apply_to_running_session`。
* 当前热更新主要比较 `capability_keys`，没有完整比较 `FlowCapabilities.excluded_tools`、MCP server fingerprint、运行时工具集合。
* 当前能力变更通知主要通过 connector steering 消息注入，不是持久化 session event；非进程内 connector 的默认实现可能静默 no-op。
* 后续 workflow step 的变化不只会影响工具能力，也可能影响特殊 mount 的装载/卸载，例如某些 step 临时挂载只读资料、产物目录、外部 provider、或撤销上一阶段的敏感资源。
* `Capability` 作为顶层命名本身是清晰的，表达“Agent 当前能做什么、能接触什么、被允许什么”。问题在于旧模型里 `capability` 基本被 tool capability key 占据了，导致顶层能力模型和工具能力子模型同名混用。

## 概念关系

本任务里的 `Capability` 应回到顶层业务概念：它表达一个 step 在当前 session 内赋予 Agent 的完整可行动边界。`CapabilitySurface` 是这个能力模型在某个 turn 上解析、组合之后实际生效的能力表面。建议分层理解：

* `Capability Model` 是顶层能力模型，描述 workflow step 希望赋予 Agent 的工具、资源、上下文和策略能力。
* `CapabilitySurface` 是能力模型解析后的生效表面，描述当前 turn 实际可见的工具表、VFS、MCP、上下文和策略状态。
* `ToolCapability` 是工具语义能力，例如 `file_read`、`workflow_management`、`mcp:code_analyzer`。
* `ToolSurface` 是工具能力解析后的投影层，包含 `FlowCapabilities`、工具级裁剪、MCP 注入和最终运行时工具集合。
* `ResourceCapability` / `VfsCapability` 是资源访问能力，描述哪些 mount 可见、各 mount 允许哪些操作、默认读写目标是什么。
* `VfsSurface` 是资源能力解析后的投影层，包含 mounts、mount links、默认 mount、每个 mount 的 read/write/list/search/exec 能力，以及 step-specific mount overlay。
* `ContextCapability` / `PolicyCapability` 可作为未来扩展，表达 context bundle、instruction overlay、permission policy、resource budget 等非工具但会影响 Agent 行为的能力。

`MCP`、工具裁剪和 `tool capability` 不宜视为三套互不相关的能力系统，而应视为工具能力在声明、解析和投影三个阶段的不同表达：

* `ToolCapability` 是上层语义声明，例如 `file_read`、`workflow_management`、`mcp:code_analyzer`。
* 工具裁剪是 `ToolCapability` 解析后的工具级结果，例如启用 `file_read` 但排除 `fs_grep`。
* `MCP` 是工具来源和 transport/provider，例如平台 Workflow MCP、project preset MCP、agent inline MCP。它通常由 `ToolCapability` 解析产生，但也需要被 `ToolSurface` / `CapabilitySurface` 记录，方便 diff、热更新和通知。

`VfsCapability` 与 `ToolCapability` 类似，都是顶层 `Capability Model` 的组成部分；区别是它描述资源空间而不是工具集合。`VfsSurface` 则是这些资源能力实际投影到运行时后的 mount 状态。挂载工作空间、挂载 lifecycle artifact、挂载 canvas、挂载外部 provider，本质上都通过 VFS 统一访问，但它们的 provider 类型、生命周期、权限边界和默认读写语义不同：

* 工作空间 mount 通常是项目或本地文件空间，生命周期较长，读写权限受 workspace/mount 配置约束。
* lifecycle artifact mount 通常绑定某个 run/step，用于 step 间产物交接，生命周期跟随 lifecycle run。
* canvas mount 是应用内托管的可视化/结构化资产空间，不应被误解为普通文件系统目录。
* 外部 provider mount 可能来自远程服务或未来插件，能力协商、缓存、错误恢复和审计要求会不同。

因此，PhaseNode transition 应先被建模为一次顶层 `Capability Model` 的变更，再解析为 `CapabilitySurface` 变更：既可能改变工具能力，也可能改变 MCP、工具表、VFS/mount、context overlay 和策略约束。通知体系也应围绕顶层能力表面建模，而不是只围绕 tool capability key。

## 需求

* 将 PhaseNode 模式支持纳入本任务整体目标：PhaseNode 不创建新 session，而是在当前 session 内完成 step contract、顶层能力模型和能力表面的可靠切换。
* 引入或明确顶层 `Capability Model`，并区分声明模型与生效表面：
  * `ToolCapability` / tool capability directives
  * `ResourceCapability` / VFS mount directives
  * `ContextCapability` / context injection 或 instruction overlay
  * `PolicyCapability` / permission policy、resource budget 等约束
* 引入或明确 `CapabilitySurface` 作为能力模型解析后的生效表面，至少覆盖：
  * effective tool capability keys / tool capability directives
  * `FlowCapabilities.enabled_clusters`
  * `FlowCapabilities.excluded_tools`
  * runtime MCP server 列表及 fingerprint
  * VFS / mount 列表、mount links、默认 mount、step-specific mount overlay
  * 可选的运行时工具名 / tool fingerprint
  * 可选的 context bundle / instruction overlay fingerprint
  * 触发来源，例如 lifecycle run、step、workflow、phase node、turn
* 设计一套更清晰、可拓展的 step capability 配置协议，不局限于旧的 `capability_directives`：
  * tool capability add/remove/tool-level whitelist/blocklist
  * MCP add/remove/replace 或 preset 引用
  * mount add/remove/replace/link/default_mount 变更
  * 未来可扩展到 context injection、permission policy、resource budget 等维度
* 梳理并逐步迁移旧模型中被 tool capability 占据的命名；原则是“顶层 Capability 保留为业务总概念，对外契约谨慎兼容，内部模型先变清楚，旧名保留 deprecated alias 过渡”。
* PhaseNode 目标能力计算必须复用 `CapabilityResolver` 的正式语义，不能只对 workflow directives 做 reduce 后当成完整目标集。
* 热更新运行中 session 时，必须同步更新：
  * hook runtime 当前能力表面
  * active turn 的 `flow_capabilities`
  * active turn / session frame 的 VFS/mount 状态
  * runtime MCP server 列表
  * connector 内的工具表
  * session profile / continuation 的能力表面缓存
* 工具级 directive 变化必须触发热更新，例如 `file_read::fs_read` 白名单变化、`remove file_read::fs_grep`、同 capability key 下 excluded tool 变化。
* mount 级变更必须触发热更新和通知，例如某个 step 增加 `lifecycle://...` 以外的特殊 mount，进入后续 step 后撤销该 mount，或切换默认写入目标。
* PhaseNode entry step、successor PhaseNode、terminal callback 等路径不能丢弃激活结果；如果当下没有 live hook runtime，需要有 pending transition 或明确的降级策略。
* 能力变更通知拆分为结构化事件和 Agent steering 两层，并在事件内表达顶层 Capability 变化和 CapabilitySurface 变化：
  * 结构化事件持久化到 session event stream，供 UI、审计、回放使用。
  * steering 消息以尽力投递方式注入运行中 Agent，并记录是否成功投递或被 connector 忽略。
  * 事件 payload 应能表达 ToolCapability / MCP / tool / VfsCapability / mount / context overlay 的 added、removed、changed、unchanged 摘要。

## 验收标准

* [x] `cargo test -p agentdash-application capability::pipeline_tests --lib` 可以运行并通过。
* [x] `cargo test -p agentdash-application step_activation --lib` 可以运行并通过。
* [ ] 新增测试覆盖 PhaseNode 作为 entry step 和 successor step 时，能够在同一 session 内应用顶层能力模型和能力表面切换。
* [x] 新增测试覆盖 PhaseNode 目标 workflow 只声明 `workflow_management` 时，默认 owner baseline 能力不会被错误清空。
* [x] 新增测试覆盖同一 capability key 内的工具级 directive 变化会触发工具表重建。
* [x] 新增测试覆盖 step-specific mount add/remove/change 会触发能力表面 diff 和工具/VFS 状态更新。
* [x] 新增测试覆盖 `replace_runtime_mcp_servers` 或替代入口使用新的 `FlowCapabilities` 重建运行时工具。
* [x] 新增测试覆盖 PhaseNode 激活后，即使当前没有 live turn，也会持久化 pending transition，并在下一次 prompt 前应用能力表面。
* [x] 新增测试或事件快照覆盖能力表面变更结构化事件被持久化，并可从 session event history 重放。
* [x] PiAgent connector 中能力变更 steering message 仍可注入当前 agent；非支持 connector 不应静默伪装成功。
* [x] 形成命名迁移清单，明确哪些旧名保留、哪些旧名废弃、哪些旧名迁移到 Capability Model / CapabilitySurface / ToolCapability / VfsCapability 语义。

## 技术方案

推荐以小 PR 串行推进，先修一致性内核，再扩展顶层能力模型与能力表面，最后补可观测性。

### PR1：PhaseNode 基线测试与现状确认

* 为现有 capability resolver、step activation、PhaseNode delta 路径补最小回归测试。
* 明确 PhaseNode entry / successor / terminal callback 三类触发路径当前哪些能工作、哪些需要 pending transition。
* 明确 review 中的风险点哪些是当前行为，哪些是 PhaseNode 完整实现前的 pending gap。

### PR2：引入 Capability Model 与 CapabilitySurface

* 引入顶层 `Capability Model`，把 `ToolCapability`、`VfsCapability`、`ContextCapability`、`PolicyCapability` 等维度明确分层。
* 引入 `CapabilitySurface` 或等价内部结构，作为能力模型解析后的完整生效表面和 diff 单位。
* 将旧 `CapabilityDelta` 从仅比较 tool capability key 集合，升级为更完整的 capability/surface delta。纯工具能力差异可进一步收窄为 `ToolCapabilityDelta`。
* 让 PhaseNode target capability/surface 通过统一 resolver / composer 计算，而不是手写 `target_capability_keys`。
* 同步整理旧模型命名，优先让内部类型、事件名、函数名表达真实语义；必要时保留旧 API 字段作为兼容 alias。

### PR3：原子化应用 CapabilitySurface

* 收口 `apply_to_running_session` / `replace_runtime_mcp_servers` / VFS update 为一个“应用 CapabilitySurface”的入口。
* 重建工具时使用目标 `FlowCapabilities` 和目标 VFS/mount，并同步 active turn、session profile、MCP servers、hook runtime revision。
* 处理无 live runtime 时的策略：pending transition、明确错误、或延迟到下一 turn apply。

### PR4：结构化通知与前端入口

* 新增持久化的能力表面变更事件。
* 保留 Agent steering Markdown，但将其视为投递通道，而不是唯一事实源。
* 前端 session timeline / workspace panel 后续可读取结构化事件，展示 phase transition card、能力 diff、MCP diff、mount diff 和投递状态。

## 决策记录

**背景**：Workflow / Lifecycle 未来会承担 Agent 工作阶段切换职责，尤其是 PhaseNode 希望在同一 session 内切换 contract、工具、MCP、mount 和行为约束。`Capability` 作为顶层业务概念是合适的，但旧模型把这个词收窄到了 tool capability key。如果只更新部分字段，会出现“模型以为有能力但工具没有”“工具还在但通知说移除”“mount 已撤销但系统上下文还在引用”“UI 历史无法回放”等漂移。

**决策**：保留 `Capability` 作为顶层能力模型命名，将 step 切换建模为 `Capability Model` 的状态转换；再将其解析为 `CapabilitySurface`，并让所有运行时入口共享同一个 compose/diff/apply/notify 流程。工具能力只是顶层 capability 的一个维度，mount/MCP/context overlay 都应进入同一套能力变更模型。

**影响**：短期实现会比只修单点 bug 稍重，但可以避免 PhaseNode、AgentNode、Task session、companion workflow、特殊 mount 装载等路径继续分叉。后续也能自然支持 UI diff、回放、policy guard、dry-run 预览和 step-level resource overlay。

## 非目标

* 不在本任务内重新设计完整 Workflow/Lifecycle DAG 语义。
* 不在本任务内引入循环、条件分支、fork/join policy 等新编排语义。
* 不在本任务内实现所有可能的 mount provider；本任务关注配置模型、diff/apply/notify 管线，以及至少一个可测试的 step-specific mount add/remove 场景。
* 不在本任务内要求所有第三方 connector 都支持 live tool hot-swap；但必须有清晰的 capability 声明、降级行为和可观测状态。
* 不在本任务内做前端大型交互改版；前端可以先消费结构化事件做最小展示。

## 技术备注

### 已落地进展（2026-05-06）

* `CapabilitySurface` 已成为 session 层可持久化类型，当前覆盖 `FlowCapabilities`、MCP server 列表和 VFS/mount 表。
* PhaseNode live apply 路径已按完整 `CapabilitySurface` 比较并应用，不再只比较 capability key 集合；同 key 内的工具级裁剪会触发热更新。
* PhaseNode 激活时若 root session 没有 active turn，不再只产生 warning，而是将解析后的 `PendingCapabilitySurfaceTransition` 写入 `SessionMeta`；下一次 prompt 进入 pipeline 时会消费队列、应用最后一个 surface、清空 meta，并持久化 `capability_surface_changed` 事件。
* pending surface 中的 lifecycle mount 会叠加到当前/默认 VFS 上，保留 workspace 默认 mount，避免 PhaseNode 切换把工作区 mount 错误清空。
* 结构化能力表面事件已进入 session event stream；live steering message 仍是尽力投递通道，并在事件中记录 delivery status。
* PostgreSQL / SQLite session repository 已增加 `pending_capability_surface_transitions_json` 字段和迁移。
* `CapabilityConfig` 已成为 workflow contract 与 lifecycle step 的顶层能力配置载体；旧 `capability_directives` 继续表达工具能力维度，新 `mount_directives` 表达 VFS/mount 资源能力维度。
* `MountDirective` 已支持 add/remove/replace mount、add/remove link、set default mount，step 级配置会在 workflow contract 配置之后应用。
* Workflow 管理 MCP 的 upsert schema 已允许 workflow contract 与 lifecycle step 传入 `capability_config`，避免配置模型只停留在内部类型。
* `session::capability_surface` 已收口 VFS overlay 合成、CapabilitySurface 多维 diff 与 `capability_surface_changed` 事件 payload 构建；live apply、pending transition、next-turn apply 共用同一套事件结构。
* 结构化事件的 `delta` 已按 ToolCapability、tool surface、MCP、VFS/mount/default mount 分维度表达 added / removed / changed；旧 `added/removed/capabilities` 字段暂保留为工具能力摘要。

仍待继续：

* 补 PhaseNode entry step 与 successor PhaseNode 在 start-run / terminal callback 两类入口下的更高层端到端测试。当前底层 apply/pending/next-turn 语义已覆盖，但 API/orchestrator 级联动仍值得单独补一层 fixture。
* 将 `CapabilityConfig` 继续扩展到 context overlay、permission policy、resource budget 等维度，并让 `CapabilitySurfaceDelta` 对这些维度也给出 changed 摘要。

### 命名迁移候选

以下不是一次性强制改名清单，而是实现时需要逐项评估的语义收口方向：

* `CapabilitySurface`：作为顶层能力模型解析后的生效表面保留，当前已经覆盖 Tool/VFS/MCP，后续扩展 Context/Policy；若某个类型只描述工具能力，应命名为 `ToolSurface` / `ToolCapabilitySurface`。
* `CapabilityConfig`：作为 workflow contract 与 lifecycle step 的声明式顶层能力配置保留，当前先承载 `mount_directives`；后续 context/policy/resource budget 继续进入这里，而不是塞进工具能力字段。
* `MountDirective`：作为 VFS/mount 资源能力变更的正式指令保留，覆盖 add/remove/replace/link/default mount。未来若资源能力超出 VFS，可再抽 `ResourceCapabilityDirective`，但当前不急于改名。
* `CapabilityDelta`：保留给 hook runtime 的 tool capability key 集合变化，不再承担完整能力表面 diff；顶层表面差异使用 `CapabilitySurfaceDelta`。
* `CapabilitySurfaceDelta`：作为结构化事件与 UI/审计消费的多维 diff，当前覆盖 ToolCapability、tool surface、MCP、VFS/mount/default mount。
* `CapabilityChanged` / capability changed hook：保留为顶层能力变更 hook，但 payload 必须来自 `CapabilitySurface` 事件结构；若未来只触发工具能力变化，可另增 `ToolCapabilityChanged`。
* `capability_directives`：保留为工具能力维度的历史字段，语义上等价于 `tool_capability_directives`；新增能力维度不再扩进该字段，而是进入 `CapabilityConfig`。
* `FlowCapabilities`：保留表示内置工具 cluster、工具级裁剪和 effective tool capability keys；MCP、mount、context overlay 不混入其中。
* `replace_current_capability_surface`：作为完整能力表面热更新入口保留；旧 `replace_runtime_mcp_servers` 语义应避免继续扩张，MCP 替换只是表面应用的一部分。
* 通知文案中的 “Capability Update”：作为顶层能力变更文案可以保留，但结构化事件必须分维度展示 Tool / MCP / VFS / Context / Policy；中文统一称“能力表面变更”。

评审关联文件：

* `crates/agentdash-application/src/workflow/orchestrator.rs`
* `crates/agentdash-application/src/workflow/step_activation.rs`
* `crates/agentdash-application/src/session/assembler.rs`
* `crates/agentdash-application/src/session/hub/tool_builder.rs`
* `crates/agentdash-application/src/session/hub/hook_dispatch.rs`
* `crates/agentdash-application/src/capability/resolver.rs`
* `crates/agentdash-application/src/capability/notification.rs`
* `crates/agentdash-application/src/vfs/`
* `crates/agentdash-domain/src/workflow/value_objects.rs`
* `crates/agentdash-spi/src/connector/mod.rs`

相关规范：

* `.trellis/spec/backend/capability/tool-capability-pipeline.md`
* `.trellis/spec/backend/workflow/lifecycle-edge.md`
* `.trellis/spec/backend/vfs/vfs-access.md`
* `.trellis/spec/backend/session/session-startup-pipeline.md`

已收纳 Review 发现：

* P1：PhaseNode 目标 capability 集合会丢失 owner 默认 baseline。
* P1：运行时工具重建使用了过期的 `FlowCapabilities`。
* P1：工具级 capability directive 变化无法被仅比较 key 的 delta 感知。
* P2：首个 PhaseNode 激活结果可能被 start-run / callback 路径丢弃。
* P2：能力变更通知未持久化，并且可能静默 no-op。
* 新增范围：step-level capability changes 可能包含 mount add/remove/change，所以模型必须比 tool capability keys 更宽。
* 新增范围：旧模型命名需要随 Capability Model / CapabilitySurface / ToolCapability / VfsCapability 分层一起收口，避免概念债继续扩散。

## 后续展望

* 在 Workflow 编辑器中提供能力表面 dry-run 预览，保存 workflow/lifecycle 前先展示每个 step 的表面变化。
* 在 session timeline 中展示 phase transition card，包含 step 切换、capability 增删、MCP/tool/mount 变化和投递状态。
* 从 session event stream 回放 CapabilitySurface，使恢复后的 session 能重建每个历史 turn 当时的工具和 mount 表面。
* 支持 policy guard，例如“review phase 不得拥有 `file_write`”或“apply phase 必须包含 `workflow_management`”。
* 支持 pending PhaseNode transition queue，应对 lifecycle 状态变化时目标 session 不在运行中的情况。
* 支持 step-level resource overlay，让 workflow 作者声明式地给某个阶段挂载临时 project snapshot、artifact、外部 provider mount 或受限资源范围。

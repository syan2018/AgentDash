# Capability 维度管线标准化

## Goal

把能力系统从“每新增一个能力维度，就在多层数据结构里挨个加字段”的模式，收束为“统一主干 + 可注册维度模块”的模式。

主干只负责声明、贡献、运行态效果、最终投影这四类通用 artifact 的生命周期；tool、MCP、companion、VFS/mount、Skill/guideline/runtime surface 等具体能力由各自 dimension module 描述和处理。新增能力时应优先新增一个 dimension module，而不是修改 `RuntimeContextPatch`、resolver input、construction DTO、context DTO 等一长串结构。

本任务采用替换式重构：先把现有 tool、MCP、companion、VFS 的核心解析与 replay 逻辑拆成内置 dimension modules，再把生产链路切到新 modules，完成后旧 runtime context patch 链路不再作为生产路径存在。这样新模型不是旧链路旁边的一层适配，而是能力主链路本身。

## Current Chain

当前链路大致是：

```text
Workflow / Lifecycle CapabilityConfig
  -> StepActivation 合并 tool_directives / mount_directives
  -> CapabilityResolverInput / ContextContributions 归约 tool + MCP + companion
  -> RuntimeContextPatch 平铺保存 tool_directives + tool/mcp/companion/vfs intent
  -> runtime command store payload_json
  -> construction finalize replay patch
  -> normalize_capability_state_dimensions
  -> CapabilityState / VFS / MCP / Skill baseline / guidelines / runtime_surface
  -> LaunchExecution / ExecutionContext / /sessions/{id}/context
```

这条链路的问题不是阶段数量本身，而是维度知识横切了太多结构：

- `RuntimeContextPatch` 要为每个维度加字段；
- replay helper 要为每个维度写分支；
- construction finalize 要知道 pending MCP/VFS 怎么取；
- specs/tests/search gates 要逐个维度补；
- plugin/extension 想加入能力时，容易变成跨层改一串字段。

上一轮把 payload 从 projection cache 收成 typed intent 是必要的，但仍然没有解决“新增维度需要改主干结构”的扩展性问题。继续在旧链路旁边并行放置新模块会让两套语义长期并存；本任务要把旧 patch 链路整体替换为 dimension registry 链路。

## Target Model

目标不是制造更长的 pipeline，而是把 pipeline 压成一个稳定主干：

```text
CapabilityDeclarationRecord[]
  -> CapabilityContributionRecord[]
  -> RuntimeCapabilityEffectRecord[]
  -> CapabilityProjectionRegistry
```

主干只认识 envelope：

```text
dimension_key: "tool" | "mcp" | "companion" | "vfs" | plugin-defined
artifact_kind: declaration | contribution | effect | projection
operation/type: dimension-owned string
payload: dimension-owned typed payload or validated JSON
```

维度模块负责：

- 声明自己支持的 declaration/effect 类型；
- 将 record payload 解析为模块内强类型 payload；
- 将 declaration 和 runtime facts 编译为 contribution；
- 将 contribution 归约为 runtime effect 或 final projection；
- replay 自己的 runtime effect；
- 参与 final projection normalization；
- 提供测试 fixture 和 schema/validation。

## Naming Standard

| 层级 | 标准名 | 含义 |
| --- | --- | --- |
| Declaration | `CapabilityDeclarationRecord` | 业务或配置层声明的能力意图 |
| Contribution | `CapabilityContributionRecord` | 带来源身份、授权语义、候选数据的归约输入 |
| Effect | `RuntimeCapabilityEffectRecord` | runtime command 可 replay 的执行效果 |
| Projection | `CapabilityProjection` / dimension-specific projection | 闭包后的 connector/UI/model 输出 |
| Dimension Module | `CapabilityDimensionModule` | 一个能力维度的声明、归约、replay、投影实现单元 |

`source` 和 `effective` 只作为解释性形容词，不作为核心类型名。`Intent` 只用于用户输入或高层请求，不用于 runtime payload 的混合结构。`Patch` 只用于真正字段补丁，不用于能力迁移语义。

## Confirmed Facts

- `CapabilityConfig` 当前显式承载 `tool_directives` 与 `mount_directives`，它们本质上是 declaration。
- `ToolCapabilityDirective` 可表达平台能力、工具级能力、以及 `mcp:<server>` 能力。
- `MountDirective` 是 op-style declaration/effect，天然适合按顺序重放到 VFS。
- `CapabilityResolverInput` 当前使用 `ContextContributions { source, tool, companion }` 与独立 `McpCandidates` 归约 tool / MCP / companion。
- MCP 当前没有独立 declaration 类型，而是通过 `ToolCapabilityDirective("mcp:<server>")` + `McpCandidates` 解析为 `SessionMcpServer`。
- Companion 当前没有 declaration 形态，主要通过 `CompanionContribution { available }` 进入 resolver，并在 runtime payload 中以 set-agents effect 保存执行效果。
- VFS/mount 当前不主要走 `CapabilityResolver`，而是在 construction finalize / runtime command replay 阶段合并 owner/session/runtime facts。
- Plugin / extension 相关规划已经要求 runtime extension asset 可声明 capability directives、MCP preset、slash command、flag、renderer 等能力，但当前 session construction 主要产出只读 `extension_runtime` metadata projection，还没有统一接入可注册 capability dimension 管线。

## Requirements

- 定义 Capability 维度管线的统一术语与边界：declaration、contribution、effect、projection、dimension module。
- 设计稳定 envelope 结构，让 runtime command payload 能保存一组 effect records，而不是每个维度一个顶层字段。
- 设计 dimension module registry，主干通过 registry 分发 validation、replay、projection normalize。
- 先拆出内置 dimension modules 的核心解析与 replay 逻辑，再替换生产链路。
- 完成后 production runtime command replay 只走 dimension registry，不保留旧 `RuntimeContextPatch` replay 作为并行路径。
- 梳理现有维度矩阵，至少覆盖 tool、MCP、companion、VFS/mount、Skill baseline、guidelines、runtime surface、extension runtime。
- 明确每个维度当前是否已能模块化接入；暂未完整模块化的 projection-only 维度要进入 registry 矩阵，而不是把字段继续扩散。
- runtime command payload 不保存完整 `CapabilityState`、`ToolDimension`、`CompanionDimension` 或 runtime surface projection。
- replay 主入口只遍历 effect records，并按 dimension module 分发处理。
- construction / context query / next-turn launch / pending apply event 继续共享统一 replay + projection normalizer。
- 多个 requested runtime command 必须按创建顺序 fold replay 到 construction base projection；VFS/mount operation 不能只读取最后一个 pending transition。
- 内置 dimension module 的 envelope payload 必须在 module 边界 decode 到强类型 payload 并执行 validation；`serde_json::Value` 只作为主干 envelope 的持久化容器。
- 新增能力维度必须通过 dimension module 接入，不能要求主干结构新增维度字段。
- 更新 backend specs，让 future agent 能判断新增能力是否违反模块边界。
- 补充测试，证明 payload 是 record/envelope 形态，且 replay 后 final projection 与现有行为等价。

## Acceptance Criteria

- [ ] `.trellis/spec/backend/capability/` 或 session spec 中新增/更新 Capability 维度管线规范。
- [ ] 规范包含 declaration、contribution、effect、projection、dimension module 的定义和判定规则。
- [ ] 规范包含现有能力维度矩阵，并标注每个维度是 built-in module、projection-only module 还是 future module。
- [ ] runtime command payload 类型从维度字段平铺改为 declaration/effect records 或等价 envelope 结构。
- [ ] replay 入口通过 dimension key 分发到 registered module，而不是主干手写每个维度字段。
- [ ] tool、MCP、companion、VFS 的 record validation / decode / replay 逻辑位于对应 dimension module。
- [ ] construction / context query / next-turn launch / pending apply event 使用同一个 replay 函数按顺序应用所有 requested transitions。
- [ ] 旧 `RuntimeContextPatch` / `Runtime*Intent` replay 生产链路被移除，生产路径只使用 `RuntimeCapabilityTransition` records。
- [ ] 生产代码不再出现新的 full projection -> runtime payload 反推路径。
- [ ] serialization / repository 测试断言 payload 不含 `state`、`tool`、`companion` replacement，也不含 runtime surface / skill baseline projection。
- [ ] runtime/context/launch 聚焦测试仍证明 pending VFS/MCP/tool/companion 顺序 replay 后 final projection 与现有行为等价。
- [ ] built-in module 测试覆盖非法 payload decode / validation error，证明错误停在 module 边界。
- [ ] plugin/extension 规划边界更新：extension 新能力应产出 declaration/effect records 或注册 dimension module，而不是要求修改主干 DTO。
- [ ] 不修改 runtime command 数据库表结构。

## Out of Scope

- 不一次性重写整个 `CapabilityResolver`。
- 不要求本轮把所有现有维度都改成动态第三方 plugin module；首版以内置 dimension modules 完成替换式重构。
- 不实现第三方动态加载 Rust capability module。
- 不改变 connector tool hot update 的外部行为。
- 不重设计前端 WorkspacePanel；上一轮已完成 current runtime state 收束。

## Open Questions

- envelope payload 在 Rust 内部采用 `serde_json::Value + module validation`，还是中心 enum？推荐主干使用 envelope + validated JSON，内置模块内部保持强类型转换，这样才能避免每加维度就改中心 enum。
- `CapabilityResolver` 是本轮就拆成 dimension registry，还是先在 runtime command replay 层引入 registry？推荐先从 runtime command replay 层切入，风险更低，同时为 resolver 后续拆分留接口。

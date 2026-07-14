# 恢复 Agent Runtime ContextFrame 平台投影链路

## Goal

补齐 `07-10-agent-runtime-architecture-convergence` 删除旧 `application-runtime-session` 时遗漏的 ContextFrame 平台生产链，使 AgentFrame 及其运行期 surface/context 变化重新以 canonical presentation 事实进入 Managed Agent Runtime，并在不改变既有前端会话行为的前提下恢复 main-reference 的 ContextFrame 内容、顺序与聚合边界。

## Background

- 07-10 的目标架构规定：Application source adapters 提供 AgentFrame、Workflow、Memory、VFS、Skill、MCP、Permission、Hook 等 typed facts；Business Agent Surface 编译 `ContextRecipe`、context contribution 和 immutable surface；Managed Runtime 持久化 context、HookRun 与 presentation；Native/Codex/Remote adapter 只翻译已绑定 surface。
- main-reference 的 `agentdash-application-runtime-session` 在启动和运行期 surface transition 时构造 ContextFrame，并通过 `SessionMetaUpdate { key: "context_frame" }` 持久化到会话流。
- 当前前端 `packages/app-web/src/features/session` 的 ContextFrame 解析、分组与渲染仍与 main-reference 对齐；`agentdash-application-agentrun::context_projection` 也仍读取 `context_frame`。
- 当前生产代码没有 ContextFrame producer：`CanonicalRuntimeSurfaceAdopter` 提交 `SurfaceAdopt` 时使用空 presentation；`ManagedAgentRuntime` 只管理 context checkpoint/compaction；API 的 `AgentFrameNativeSurfaceCompiler` 直接构造 Driver surface，未使用 `agentdash-agent-runtime::surface::AgentSurfaceCompiler` 的 `ContextEnvelope` / `ContextContribution`。
- 当前工具调用仍可工作，因为工具 schema 与执行代理走独立链路：Application `RuntimeToolProvider` 组装工具，API surface compiler 生成 `DriverToolSurface`，Native adapter 将其转换为 Agent Core tools，调用时通过 Host callback 回到 Platform Tool Broker。ContextFrame 的 `ToolSchemaDelta` 只是展示/审计投影，不是工具可调用性的事实源。

## Requirements

### R1. 平台所有权

- ContextFrame 的业务构造属于 Business Agent Surface / Managed Runtime 边界，不得放入 Native、Codex 或 Remote adapter。
- Adapter 只消费 `MaterializedDriverSurface`，不得读取 AgentFrame repository、业务 capability policy 或前端 presentation 类型。
- API composition 只装配 source adapters、surface compiler、runtime 与 host，不继续拥有 Native 专用业务 surface 编译逻辑。

### R2. 单一编译结果

- 同一次 AgentFrame/surface 编译必须从同一组 typed facts 派生：
  - `ContextRecipe` / materialized context；
  - instruction 与 tool/workspace/hook surface；
  - 面向会话流的 ContextFrame projection。
- ContextFrame 不得反向从 Driver DTO、tool callback 或前端 read model 猜测。
- `FrameContextBundleSummary` 继续只表达控制面摘要，不得作为模型用户输入或伪装成完整 context contribution。
- main-reference 只作为外部行为与业务规则 oracle；允许重构内部事实类型、projection pipeline、delta 推理、ID/timestamp 注入和模块组织，但不得改变用户可观察 payload 语义。

### R3. 初始与运行期投影

- 新 Runtime Thread 的 bootstrap ContextFrame 必须随首个 canonical `ThreadStart` presentation 原子提交。
- 已有 Runtime Thread 的 AgentFrame/surface 变化必须随 `SurfaceAdopt` presentation 原子提交。
- 无业务变化的幂等 adoption 不产生重复 ContextFrame。
- ContextFrame presentation 的失败必须使对应 canonical mutation 失败，不允许 surface 已采用而展示事实缺失。

### R4. main-reference 行为等价

- 除允许替换 event stream 外层 wrapper/coordinate 外，ContextFrame payload、frame 顺序、frame 内 section 顺序、空值语义、相邻 frame 聚合边界必须与 main-reference 一致。
- 前端 `packages/app-web/src/features/session` 不改交互、布局、分组策略或文案；只允许适配 canonical wrapper 的等价解析。
- 至少覆盖 bootstrap、assignment、capability/tool schema、Skill、Memory、VFS/MCP、identity/user/environment/guidelines、pending action/auto-resume、compaction summary 等 main-reference 已支持 frame family。

### R5. Runtime revision 语义

- Durable ContextFrame 随所属 canonical operation 推进 Runtime thread revision，不单独建立第二写者。
- ContextFrame 只有在 materialized context/head 确实变化时才推进 context revision；展示事件本身不得伪造 context head 变化。
- ToolCall started/terminal 可作为 canonical transcript item 推进 thread revision；纯 progress update 不推进 durable revision；工具是否无外部状态不影响 transcript lifecycle 的持久性。

### R6. 类型与边界清理

- 将 ContextFrame 的 canonical owned 类型放到平台/runtime presentation 合同的合适位置，避免继续由 `agentdash-spi::hooks` 偶然拥有。
- 不增加 compatibility path、dual write、fallback producer 或按 adapter 类型分流。
- 不恢复已删除的 `application-runtime-session` mega-module；复用其业务规则时按新边界拆入 source adapter、Business Agent Surface 和 Managed Runtime。

### R7. 验证

- 建立 main-reference golden/oracle fixtures，逐 payload 比较初始与 live transition event stream。
- 建立 `ThreadStart`、`SurfaceAdopt` 的 Runtime journal/presentation 原子性测试。
- 建立 Native、Codex、Remote adapter 不生产 ContextFrame 的边界测试。
- 建立工具 schema 注入与 ContextFrame presentation 相互独立但同源编译的测试。
- 真实 `pnpm dev` 新会话验证 ContextFrame 卡片、工具调用和运行期 surface 更新。

### R8. 强制参考源与整体实施

- 固定使用只读参考仓库 `D:/Projects/AgentDash-main-reference`，基线 commit 为 `957fa9d60ea3d67efa1bb278fe5b376cf0c34598`。
- 任何 frame family 的规则实现前，必须先在参考仓库定位 builder、触发点、排序、去重与持久化路径，并把证据写入任务 research/golden。
- 参考仓库用于搬运业务规则和验证行为，不直接复制旧 `application-runtime-session` 模块边界。
- 本任务一次恢复 main-reference 的全部 ContextFrame family；不只修 live surface update。
- 保持单一主任务，不拆 child；实施工作项保持少量、端到端、可整体 review，避免按文件或层级拆成碎片。

## Acceptance Criteria

- [ ] AC1：生产 composition 调用统一 Business Agent Surface compiler，不再由 API 的 Native 专用 compiler 独立拼装业务 surface。
- [ ] AC2：首个 ThreadStart 的 ContextFrame payload 与 main-reference golden 完全一致，外层 wrapper 差异经过显式映射。
- [ ] AC3：AgentFrame runtime surface adoption 的 ContextFrame payload与顺序和 main-reference live transition golden 完全一致。
- [ ] AC4：ContextFrame 与所属 canonical mutation 在同一 Runtime UoW 中提交；失败与重放均不产生半提交或重复展示。
- [ ] AC5：Native/Codex/Remote adapter 代码中不存在 ContextFrame 业务构造或 AgentFrame repository 读取。
- [ ] AC6：工具 schema 仍通过 DriverToolSurface 注入并可执行；ContextFrame 缺失或展示策略不再影响工具可用性。
- [ ] AC7：前端 session ContextFrame reducer/feed/UI 行为相对 main-reference 无变化。
- [ ] AC8：Runtime thread/context/surface revision 的推进符合 R5，并有并发与幂等回归测试。
- [ ] AC9：相关 Rust、TypeScript、schema、golden、跨层 E2E 质量门禁通过。
- [ ] AC10：任务 research 明确记录参考仓库 commit、源文件映射与每个 golden 的原始行为证据。

## Out of Scope

- 不改变 Codex App Server Protocol 或把 ContextFrame 塞入 vendor protocol。
- 不重设计 ContextFrame UI。
- 不恢复旧 RuntimeSession、Connector 或 Backbone 双事实源。
- 不以本任务为由重写无关的 context compaction 算法或工具业务实现。

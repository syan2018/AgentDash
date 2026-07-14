# Agent Runtime ContextFrame 平台投影恢复实施计划

## 执行原则

- 单一主任务，不建立 child task。
- 只保留四个端到端工作项；每项跨越必要层级并交付可查验结果，不按 crate/文件拆碎。
- 强制对照只读参考仓库 `D:/Projects/AgentDash-main-reference@957fa9d60ea3d67efa1bb278fe5b376cf0c34598`。
- 先提取 oracle，再实现；保留 payload/顺序行为，优化内部类型、推理和模块边界。
- 不恢复旧 `application-runtime-session`，不加 compatibility/fallback/dual write。
- 当前工作区已有未提交 runtime 修复，实施时必须保留并基于其继续，不覆盖并行修改。
- 每个工作项完成后独立提交；WI-04 完成前不得归档或宣称恢复成功。

## WI-01：参考基线、owned vocabulary 与 Context Projection feature 骨架

### 目标

把 main-reference 的全部业务规则转为可执行 oracle，并一次确定 feature 的 owned 类型、纯投影接口和模块边界。

### 工作

- 按 `research/main-reference-context-frame-inventory.md` 清点所有 frame family、触发点、payload、section/order/dedupe/delivery 规则。
- 提取 bootstrap、live delta、hook/pending/auto-resume、compaction 的 builder golden 与 stream golden。
- 建立 wrapper-neutral diff harness；动态 ID/timestamp/coordinate 采用显式 normalization。
- 将 ContextFrame/delivery 类型迁到 owned platform presentation contract，增加 typed `PlatformEvent::ContextFrameChanged`。
- 建立 `agentdash-agent-runtime::context_projection` deep module：typed facts、closed enums、纯 projector、显式 identity/clock、规范化 delta 与 presentation plan。
- 现有 Rust/Application/frontend consumers 切换到 owned type，但不改变展示行为。

### 完成判据

- 每个 main-reference frame family 都有源文件证据和失败中的当前分支 oracle test。
- payload serialization 与 main-reference 一致。
- projector 不依赖数据库、时钟、connector、AgentFrame repository 或全局 notice queue。
- API/adapter 不拥有 builder。

### 建议提交

`refactor(context): 建立可重放 Context Projection 模块`

## WI-02：统一 Agent Surface 编译与不可变 artifact

### 目标

让工具、模型上下文和 ContextFrame 真正由同一次 Business Agent Surface 编译产生，并跨 provision/重启保持 exact artifact。

### 工作

- 定义 Application `context_sources` adapters，将 AgentFrame、identity、workflow、memory、VFS/MCP、Skill、Hook facts 映射到 runtime-owned facts。
- 将 API `AgentFrameNativeSurfaceCompiler` 的业务逻辑迁入统一 Business Agent Surface compiler；API 只保留 composition。
- 一次输出 `AgentSurfaceSnapshot + MaterializedDriverSurface + RuntimeSurfacePresentationPlan + publication`。
- 工具 schema、ToolSchemaDelta 与 driver tool surface 从相同 normalized tool facts 派生；ContextFrame 不参与执行路由。
- 搬运并重构 main-reference 的 bootstrap、identity/user/environment/guidelines/memory/assignment/capability/tool/Skill/VFS/MCP projection rules。
- 按 `(binding, surface revision, surface digest)` 持久化 immutable compiled artifact，并添加目标 schema migration。
- binding/recovery 引用 exact artifact；不得读取被覆盖的“当前 surface”。

### 完成判据

- 生产 composition 使用统一 compiler。
- provision 后、首条消息前重启仍能读取 exact bootstrap plan。
- 工具继续通过 DriverToolSurface 注入并能调用。
- 相同 facts 只编译一次，driver/context/presentation revision 与 digest 可相互证明。
- 空库与代表性数据库 migration 通过。

### 建议提交

`refactor(runtime): 统一 Agent Surface 与展示计划编译`

## WI-03：接通全部 canonical producer 与原子 Runtime journal

### 目标

将所有 ContextFrame family 放回其正确 canonical operation，使 surface/context/hook 状态和 presentation 不再分叉。

### 工作

- 首次 send_message 从 exact artifact 读取 bootstrap plan，与用户 submission 随 ThreadStart 同交。
- SurfaceAdopt 使用 previous accepted surface/AgentFrame 做 typed delta，随 operation 原子提交 adoption frames。
- Hook 模型可见 effect 通过 `context_projection` 生成 frame，与 HookRun/effect UoW 同交。
- pending action/auto-resume 随 mailbox/next-turn operation 提交。
- managed compaction activation 生成 compaction summary frame并与 checkpoint/head 同交；native opaque compaction仍只作 telemetry。
- 统一 idempotency/replay/empty-delta/concurrent-adoption 语义。
- Runtime thread/context/surface revision 分别遵守 PRD R5。

### 完成判据

- ThreadStart、SurfaceAdopt、Hook、pending/auto-resume、compaction stream golden 全部与 main-reference payload 序列一致。
- presentation failure 不会留下已采用 surface/head/HookRun。
- retry/replay 不重复 ContextFrame。
- 并发 adoption 不丢 frame、不串 artifact/revision。
- Native/Codex/Remote 均不生产 ContextFrame。

### 建议提交

`feat(runtime): 恢复 ContextFrame canonical 生产链`

## WI-04：前端零行为恢复、全链路差异矩阵与真实验证

### 目标

证明新模块在新架构下完整替代旧实现，而不靠局部测试或手造事件自证。

### 工作

- 仅在 session protocol normalization boundary 接入 typed wrapper；保持 reducer、feed grouping、ContextFrame UI、文案和工具 burst hard-boundary 不变。
- 对 main-reference 建立 bootstrap/live/hook/compaction 全 eventstream payload 差异矩阵。
- 审计 Native/Codex/Remote adapter、API composition、Application source、Runtime journal 的依赖方向。
- 运行 Rust/TypeScript/schema/conformance/E2E 门禁。
- 重启 `pnpm dev`，用新 AgentRun 验证初始 frame、工具调用、surface update、Hook/pending、compaction 和 revision。
- 更新最终 Trellis spec，只记录 Context Projection 的目标职责、事实源和设计依据。

### 完成判据

- payload 差异矩阵除显式 wrapper/coordinate 映射外为零。
- 前端行为相对 main-reference 为零差异。
- 工具调用和 ContextFrame 同时正常，且数据库 revision 符合契约。
- 全链路未出现旧 RuntimeSession、adapter builder、dual write 或 fallback。

### 验证命令候选

```powershell
cargo test -p agentdash-agent-runtime
cargo test -p agentdash-application-agentrun
cargo test -p agentdash-infrastructure agent_runtime_composition
cargo test -p agentdash-api agent_runtime_surface
cargo test -p agentdash-integration-native-agent
cargo test -p agentdash-integration-codex
pnpm --dir packages/app-web test
cargo check --workspace --all-targets
pnpm dev
```

按项目并行 Cargo 约束观察 build directory lock，不终止 rust-analyzer 或其他会话工作。

### 建议提交

`fix(session): 完整恢复 ContextFrame 会话行为`

## Review gates

1. WI-01：逐 family 对照 main-reference inventory/golden，未覆盖不得进入生产实现。
2. WI-02：审查 Application facts → Runtime compiler → Driver/presentation 双输出的依赖方向和 artifact durability。
3. WI-03：审查 Runtime UoW、revision、replay 与并发，不接受仅靠 CAS retry 掩盖多写者。
4. WI-04：审查完整差异矩阵和真实会话，不接受“单测通过”替代用户行为恢复。

## 回滚点

- 每个 WI 独立提交，可按工作项回滚。
- migration 尚未上线，设计错误时直接修订到目标 schema，不保留旧 schema 读取。
- typed wrapper 接入失败时修复 normalization，不恢复双格式生产。

# PRD — Capability Update Primitives 能力更新原语

## 背景

AgentFrame 是按 revision 累积的运行时快照。每次 ProjectAgent 配置变更/起 session 都基于上一版 frame 派生新 revision，需要决定每个能力维度"取新值还是继承"。当前这套 merge / 积累逻辑被摊到三层（config merge_field!、frame_builder 各字段手写 merge、运行时 intersect/旁路赋值），**每个维度规则不一致、缺少统一原语**。

直接症状：workspace_module 可见性 allowlist 一旦设过就无法在 UI 清空回"全部可见"——清空走了 carry-forward 臂，把旧名单原样继承回来（见 [research/01](research/01-current-capability-model.md) §4-5）。但这只是表症。

根因（探查确认）：项目里**已存在一套事件溯源能力原语**（`RuntimeCapabilityTransition` = declarations + effects，`CapabilityDimensionKey` / `CapabilityArtifactSource` 标识，`CapabilityDimensionModule` 重放引擎），但 6 个维度里只有 tool/vfs/mcp 走它，companion/skill/workspace_module 各走旁路。缺一个**显式的 AccumulationPolicy**，也没把声明式 base 与运行时 modifier 的边界讲清楚。

## 目标

把现有半成型的能力原语**收口成全维度唯一面**，并显式化「积累规则」，使「能力更新」「能力标识」成为一套标准化、可声明、可重放的原语。具体：

1. **分层定型**：明确 base CapabilityState（声明式真值，materialized 进 `effective_capability_json`）vs runtime modifier（`RuntimeCapabilityTransition` 重放）两层边界，所有维度统一遵守。
2. **AccumulationPolicy 显式化**：为每个维度声明积累规则（Replace / Accumulate / Ephemeral），重放引擎统一按 policy 行事，替代各 module 硬编码。
3. **三态全链路保真**：从 config 到 frame 到 runtime 不再 `unwrap_or_default` 抹平 `Unspecified`/`Cleared`/`Allowlist`；清空 = 显式回默认（全集），不再继承旧值。
4. **6 维度归类落地**：tool/vfs/mcp/companion/skill/workspace_module 全部归到统一 policy 表；退役 companion/skill/workspace_module 三条旁路，收口到 canonical 路径。

## 范围

- **In**：CapabilityState 维度模型、AccumulationPolicy 原语、frame 构建收口到统一 resolver、workspace_module/skill/companion 旁路退役、三态契约与前端清空语义、文档与契约导出。
- **Out**：不改 tool/vfs 现有 effect 语义本身（它们已 canonical，仅归类标注 policy）；不引入新的可见性维度；不改 canvas mount 的累积语义（仅作为 Accumulate policy 的既有样本归类）；不重构 permission grant 编译链（仅复用其 declaration 通道）。

## 取代关系

本任务取代 PR #45 (workspace-module-registry) Child 4 引入的临时 carry-forward 语义（frame_builder 混合 match 臂 + composer unwrap_or_default）。该临时实现在本任务落地后删除，workspace_module 可见性改由 base CapabilityState.workspace_module（mode 三态）承载。

## 决策（待 design 钉死，见 design.md 决策表）

- **DA**：base/modifier 是否需要在 AgentFrame 上**物理拆分存储**，还是 base 仍存 effective_capability_json、modifier 仍走 transition 表（倾向后者，零新表）。
- **DB**：workspace_module 声明式 allowlist 是走 base CapabilityState.workspace_module（经 effective_capability_json），还是新增 effect 类型（倾向前者，最小改动且天然修 bug）。
- **DC**：skill/companion 旁路退役的彻底程度——是全量纳入 declaration/effect 重放，还是仅纳入 policy 归类 + 统一 base 投影（倾向后者，避免对最敏感的能力门做高风险事件溯源改造）。
- **DD**：AccumulationPolicy 放 spi 契约还是 domain（倾向 spi，与现有 Capability* 标识原语同层）。

## 验收标准

1. 存在一个**显式 AccumulationPolicy** 原语，6 个维度各自归类有据可查（文档 + 代码声明）。
2. frame 构建处不再有 per-field 手写 merge 的散乱分支；可见性/能力维度经统一路径产出。
3. **清空 workspace_module allowlist → 下一 revision 回到"全部可见"**（mode=All），不再继承旧名单；全新 agent 仍默认全集。回归测试覆盖：set → clear → 验证 All；set → 保持 → 验证 Allowlist。
4. companion/skill/workspace_module 不再各走旁路赋值；其声明式真值统一从 base CapabilityState 投影（旁路代码删除）。
5. 三态在 config→frame→runtime 全链路保真：`Unspecified` 继承、`Cleared` 回默认、`Allowlist` 受限，单测覆盖三者。
6. `cargo build --workspace` + 全 workspace 测试通过；`pnpm contracts:check` 通过；`app-web` typecheck 通过；migration guard 通过（若涉及 DB）。
7. 现有 tool/vfs/mcp 行为零回归（已 canonical 的维度语义不变）。

## 子任务拆分（2 child，详见 design.md §子任务）

- **Child A**（slug `workspace-module-base-converge`）：能力原语 `AccumulationPolicy` + 6 维度归类 + workspace_module 收口 base（修 bug）+ 前端清空语义 + 契约/文档。一条完整纵切，独立交付修复价值。
- **Child B**（slug `skill-companion-bypass-retire`）：skill / companion 旁路退役，收口到统一 base 投影（最敏感，单独隔离风险）。

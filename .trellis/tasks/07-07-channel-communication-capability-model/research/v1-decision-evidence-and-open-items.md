# Channel v1 决策证据汇总与剩余三项评估方法

> **再次过期提示（2026-07-07 后续 realignment）**：本文中把 `LifecycleRun.channels: Vec<LifecycleChannel>` 标为最终决策的说明已经被推翻，D3/D5 中把一等 Channel 推向独立 `channels` / `channel_participants` 表的建议也已被后续第一性判断覆盖。最新结论见 `research/channel-service-first-principles-realignment.md` 与 `design.md`：Channel 是一等领域与 owner-scoped lazy `ChannelService` 主干；runtime Channel facts 默认进入 owner-local `ChannelRegistryDocument`，Project Channel 的物理承载等待 Assets 系统收束；participants、bindings、broadcast policy、message/delivery planning 属于 Channel facts，`CapabilityState.channel` 只作为 AgentFrame 可见操作投影。本文的代码证据仍可参考，实际实现口径以最新文档为准。

> **历史过期提示（2026-07-07 五轮后追加，已被上方 realignment 再次覆盖）**：当时 Part B 里 D3（LifecycleChannel 命名）和 D5（持久化最小 schema）的独立表建议被收窄为 `LifecycleRun.channels: Vec<LifecycleChannel>` 结构化字段。这个口径现在只作为讨论过程保留；D4（ChannelAddress 关系）的整体重定位方向仍可参考，但也需要按最新 `ChannelAddress` / delivery attribution 边界重新落地。

- 记录时间：2026-07-07（二轮对齐之后的补充研究）
- 目的：把 2026-07-07 二轮对齐时 4 个只读 Explore agent + 直接代码核实的完整证据落到 research/，并为 `implement.md` "Not Ready For Implementation Until" 剩余 3 项（`LifecycleChannel` 命名、`ChannelAddress` 与 `MailboxSourceIdentity` 关系、Channel message 持久化最小 schema）给出评估方法和具体建议。本文档只提供证据和建议，不代表已写回 `design.md`/`implement.md` 的决策——需要和 D1/D2/D5 一样经过一轮用户确认。

## 如何使用本文档

- Part A 是证据本身，按主题分类、带 `文件:行号`，供后续实现或再讨论时直接引用，不需要重新核实。
- Part B 是给剩余 3 项的评估方法：先给一个通用的"三问"评估镜片（这是这次能快速对齐 D2 的方法，值得复用），再逐项套用，给出具体建议。

---

## Part A：证据汇总

### A1. Mailbox / `MailboxSourceIdentity`

- 结构定义：`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:48-57` —— `namespace: String, kind: String, source_ref: Option<String>, correlation_ref: Option<String>, actor: String, route: Option<String>, display_label_key: String, metadata: Option<Value>`。字段名是 `metadata`，不是 `metadata_json`（design.md/journal 早期笔误）。
- 构造与方法：`impl MailboxSourceIdentity`（同文件 `:59-146`）。`new(namespace, kind, actor)` 在 `:60-69` 会**硬编码** `display_label_key: format!("mailbox.source.{namespace}.{kind}")`——这一点对 D4 很关键，见 Part B。`dedup_fragment()`（`:104-106`）只是 `format!("{}:{}", namespace, kind)`，完全通用、不含 mailbox 专属逻辑。内置工厂：`composer()/draft_start()/hook_after_turn()/hook_before_stop()/hook_auto_resume()/companion_parent_resume()/workflow_orchestrator()/routine_trigger()/local_relay_prompt()/canvas_action()`（`:108-146`）。
- Wire DTO 镜像：`crates/agentdash-contracts/src/agent/run_mailbox.rs:36-55`，字段同构（`metadata: Option<Value>`，`#[ts(optional, type = "JsonValue")]`）。
- 历史 closed enum 问题（已修复）：`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:138-149` 曾有 `agent_run_mailbox_messages_source_check`，9 个取值且缺 `canvas_action`。修复迁移：`crates/agentdash-infrastructure/migrations/0032_agent_run_mailbox_source_identity.sql`（对应 commit `f6e2406e6 feat(mailbox): 重建 source identity 模型`）整体替换为 8 个开放列（`source_namespace/source_kind/source_ref/source_correlation_ref/source_actor/source_route/source_display_label_key/source_metadata`），不再有取值 CHECK 约束；`source_metadata` 是 `text`（JSON 字符串），不是 `jsonb`。截至 `0056_gate_result_delivery_markers.sql`，没有后续迁移重新引入约束。
- 生产真实调用面：`core`（`project_agent_start.rs:2659,2910`）、`companion`（`companion/tools.rs:241-242,1107`）、`workflow`（`mod.rs:132-134`）、`routine`（`mod.rs:136-138`）。`platform` 只在 spec 与 W7 里是前瞻占位，代码中**没有**任何路径真正构造 `namespace="platform"`；对应测试 `companion/tools.rs:3135 platform_capability_grant_request_reports_missing_broker` 断言的是"缺 broker"诊断路径，不是真实投递。
- Spec 一致性：`.trellis/spec/backend/session/agentrun-mailbox.md` 与代码一致，不是过期文档（字段、PG 列、scheduler 边界描述都对得上）。
- **方法论先例**：当年 W0 阶段的全链路影响面研究记录在 `.trellis/tasks/06-28-integration-channel-mailbox-convergence/research/W0-source-identity-impact.md`，按 Domain/Infrastructure/API/Contracts/Frontend/Specs/Scheduler Evidence/Suggested Execution Order 分类，最后是**整体替换**旧 enum（不是新建通用类型再包一层）。这份文档本身就是"如何给一次值对象改造做完整影响面扫描"的可复用模板。

### A2. Capability dimension pipeline / `CapabilityState` / Workspace Module

- `CapabilityState` 定义：`crates/agentdash-spi/src/connector/mod.rs:502-518` —— 字段 `tool/companion/vfs/skill/memory/workspace_module`。
- Dimension/effect 常量：`crates/agentdash-spi/src/session_persistence.rs:197-211`。当前仅 4 个 effect_type：`set_tool_access/set_server_set/set_agent_roster/apply_vfs_overlay/apply_mount_operations`，全部是"整态替换"或"overlay 合并"语义。
- `AccumulationPolicy`：`session_persistence.rs:141-160`，三态 `Replace/Accumulate/Ephemeral`。**文档注释明确把 "canvas mount append、VFS overlay 累积" 列为 `Accumulate` 的典型场景**（`:148-149`）——这是纠正"没有 append 先例"这个过度保守判断的直接依据。
- `CapabilityDimensionModule` trait 与注册表：`crates/agentdash-application-agentrun/src/agent_run/runtime_capability.rs:281-320`（trait）、`:333-348`（`built_in()` 只注册 Vfs/Tool/Mcp/Companion 四个模块）、`:368-412`（`validate_transition`/`replay_transition`，未注册 dimension 直接报错）。
- VFS 的 `Accumulate` 实现（Channel 应直接复刻的模板）：`runtime_capability.rs:625-687`（`VfsCapabilityDimensionModule`，`policy() -> Accumulate`）、`:822-838+`（`apply_mount_directives`，按 `MountDirective::{AddMount,RemoveMount,ReplaceMount,AddLink,RemoveLink}` 对 `vfs.mounts`/`vfs.links` 做 upsert-by-id / retain-remove-by-id）。`MountDirective` 定义：`crates/agentdash-domain/src/workflow/value_objects/mount_directive.rs:11-26`。
- Companion 的 `Replace` 实现（对照组）：`runtime_capability.rs:589-623`，`replay_effect` 直接整体覆盖 `state.companion.agents`。
- Workspace Module 三段式混合体（**不是**可复刻的 projection-only 先例）：声明式部分 `WorkspaceModuleDimension`（`connector/mod.rs:441-488`）由纯函数 `project_workspace_module_dimension()`（`runtime_capability.rs:106-116`）投影，从未注册进 registry；运行时曝光部分是独立 `AgentFrame.visible_workspace_module_refs_json` 列（`crates/agentdash-domain/src/workflow/agent_frame.rs:64-71,117-133`），写入路径 `runtime_surface_update.rs:157-171,386-402`，完全绕开 `RuntimeCapabilityEffectRecord`；两者只在读取时由 `owner_bootstrap.rs:66-79` 的 `resolve_workspace_module_visibility` 做 OR 合并。`CAPABILITY_DIMENSION_WORKSPACE_MODULE`/`CAPABILITY_DIMENSION_SKILL` 常量存在但没有任何模块用它们注册进 registry。
- `intersect()`（`connector/mod.rs:638-668`）：只有 `tool` 做真正集合交集；`companion`/`vfs` 自值直传；`skill`/`memory`/`workspace_module` 显式不参与。Channel 走 `Accumulate` 应同样自值直传，不参与交集裁剪。
- 新增必填字段的历史迁移先例：`workspace_module` 字段引入时不给 `#[serde(default)]`，强制处理已持久化 AgentFrame JSON 反序列化，测试 `capability_state_json_requires_workspace_module_dimension`（`runtime_capability.rs:1412-1431`）。Channel 字段引入要重走一次同样的决策。
- `PermissionGrant` 驱动可见性目前没有通用机制：`AgentRunGrantProjection::classify_path`/`partition_paths`（`effective_capability.rs:59-85`）硬编码基于 `ToolCapabilityPath`，只服务 Tool 维度。

### A3. Companion `target=sub` / Channel 领域概念现状

- 首条任务投递确实走 Mailbox（W3 属实）：`CompanionChildDispatchService::dispatch_child`（`crates/agentdash-application/src/companion/dispatch.rs:46-165`）只创建 anchor，不落首条内容；真实投递在 `crates/agentdash-application/src/companion/tools.rs:1125-1150`，`accept_intake_message(...source: MailboxSourceIdentity::new("companion","dispatch","agent").with_route("sub")...)`，`source_ref`/`correlation_ref` 构造见 `tools.rs:1104-1123`。
- **Channel 领域概念在代码里完全不存在**：全仓库搜索 `struct Channel|enum Channel|ChannelId|mod channel` 零命中（仅命中规划文档自身）；`agentdash-domain/src` 下没有 `channel` 模块。仅有两处不相关的 "channel" 字样：`crates/agentdash-domain/src/shared_library/value_objects.rs:1503-1610`（Extension Protocol Channel，见 A4）与 `crates/agentdash-application/src/companion/reply_contract.rs:6-14`（`channel: String` 字符串标签）。
- `CompanionReplyContract` 真实字段（`reply_contract.rs:10-17`）：`route: CompanionReplyRoute, request_id: String, channel: String, aliases: Vec<String>, model_instruction: ModelReplyInstruction`；`ModelReplyInstruction`（`:88-94`）：`tool_name, minimal_arguments, reply_to: Option<ModelReplySelector>, payload_hint, text_hint`。**这套字段不是** design.md 引用的 `namespace/kind/source_ref/correlation_ref`——那套词汇属于 `MailboxSourceIdentity`，是另一个结构。
- AgentFrame 当前字段（无 channel 相关列）：`crates/agentdash-domain/src/workflow/agent_frame.rs:44-76`。已有的"暴露可见 ref 列表给模型"先例只有 `visible_canvas_mount_ids_json`/`visible_workspace_module_refs_json`，语义是"白名单 id 列表"，不是"回复地址/alias"。
- Companion 当前 reply-to/dispatch_id/gate_id 完全是运行时现算 + prompt 文本硬编码：`active_reply_targets()`（`tools.rs:1962-2044`）每次工具调用现场从 `LifecycleGate.list_open_for_agent` + pending hook actions 拼出来，从未持久化到 AgentFrame/CapabilityState；`build_companion_dispatch_prompt` 把 `ModelReplyInstruction` 渲染进 prompt 字符串（`tools.rs:2699`）。

### A4. Extension Protocol Channel 命名冲突面

- 完整链路（已上线，非命名冲突假设）：`ExtensionProtocolChannelDefinition`/`channel_key`/`protocol_channels`（`shared_library/value_objects.rs:1503-1610`）→ `agentdash-contracts/src/extension/runtime.rs`（`ExtensionRuntimeInvokeChannelRequest{channel_key, method, ...}`，有 TS codegen）→ 生成产物 `packages/app-web/src/generated/extension-runtime-contracts.ts` → HTTP `POST .../extension-runtime/invoke-channel`（`frontend-backend-contracts.md:293`）→ relay `agentdash-relay/src/protocol/extension_runtime.rs` → runtime-gateway `ExtensionRuntimeChannelInvoker`（`extension_actions.rs`）→ 权限串 `extension.channel.invoke:<channel_key>.<method>` → 前端 `services/extensionRuntime.ts`、`features/extension-runtime/model/{webviewBridge,canvasBridge}.ts`、`ExtensionCategoryPanel.tsx:624`（"N channels" UI 文案）。
- 前端全量 "channel" 用词普查（29 文件，3 组，无独立 WebSocket/SSE 通知类 channel）：① Extension protocol channel 桥接（同上）；② `EXTENSION_BRIDGE_CHANNEL = "agentdash.extension"`（`bridge.ts`，postMessage 信封 tag，非业务实体）；③ `ContextModelChannel`/`delivery_channel`（`features/session/model/contextFrame.ts` + 后端 `agentdash-spi::hooks::ContextModelChannel`，广泛用于 context frame 构造），措辞上会和 design.md 的 `ChannelDelivery` 词汇混淆，语义无关但命名相似。
- 无具体标识符冲突：design.md 提议的全部新类型名（`ChannelMessage/ChannelDelivery/ChannelCapability/ChannelOwner/ChannelMedium/ChannelTopology/ChannelAddress/LifecycleChannel/...`）在当前代码库中零命中，`agentdash-domain::channel` 模块路径未被占用。冲突是"心智模型/措辞层面"，不是编译期冲突。
- 用户决策（2026-07-07）：新 Channel 保留命名；Extension Protocol Channel 使用面不大，是重命名或收束进统一 Channel 体系（未来某个 `ChannelMedium`）的候选。不是本任务 v1 范围。

### A5.（本轮补充）`AgentRunLineage` —— 最小关系事实表的现成模板

- 领域结构：`crates/agentdash-domain/src/workflow/agent_run_lineage.rs:12-35` —— `id, parent_run_id, parent_agent_id, child_run_id, child_agent_id, relation_kind: String, parent_frame_id?, parent_frame_revision?, child_frame_id?, child_frame_revision?, fork_point_event_seq?, fork_point_ref_json?, forked_by_user_id, metadata_json?, created_at`。是"跨 run 的 provenance 关系"事实，不是事件日志——每条关系一行记录，没有多版本历史。
- 表结构：`crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:1-31`。有索引覆盖 parent/child 查询方向。**注意**：`relation_kind` 列带了 `CHECK (relation_kind = 'fork')`（第 15 行）——这是和 `MailboxMessageSource` 曾经的 closed check constraint 同一类问题，目前还没被修正，是一个可以引以为戒的活教材。
- 这是 design.md "第一版可持久化最小 channel ref / participants / binding" 这句话目前唯一能找到的同构现成模板。

---

## Part B：剩余三项如何评估

### 通用评估镜片（这次对齐 D2 时实际用的方法，建议复用）

对每个开放决策，依次回答三个问题：

1. **现有先例是什么**——代码库里有没有已经在生产环境跑的、结构相似的模式？先找模板，不要凭空设计。
2. **历史教训是什么**——这个代码库是否已经有过"选错方案后来要重构"的真实案例？（`MailboxMessageSource` closed enum 是最典型的一次教训。）
3. **长期方向是否被过早关死或过度设计**——现在的选择会不会在 Channel 扩展到 AgentTeam/Project/外部 IM 时造成额外迁移成本？还是可以先用最小形态又不堵住后路？

D2（capability 路线）就是靠"先例 = VFS Accumulate/MountDirective，教训 = 不要低估现有机制，长期方向 = 迟早要有独立 CapabilityState，不如现在就建对"这三点快速收敛的。下面把同一个镜片套在剩余 3 项上。

### D3：`LifecycleChannel` 最小实体 / 表命名

- **先例**：`LifecycleGate`/`lifecycle_gates` 和 `AgentRunLineage`/`agent_run_lineages` 是两个"lifecycle-scoped、体量小、靠一个 `_kind` 字段区分子类型"的实体。关键观察：**只有核心生命周期实体（`lifecycle_runs`/`lifecycle_agents`/`lifecycle_gates`）才带 `lifecycle_` 前缀**，`AgentRunLineage` 描述的关系同样是 lifecycle-scoped（parent/child run 都在生命周期树里），但表名和类型名都不带 `Lifecycle` 前缀，直接以内容语义命名（`lineage`）。
- **教训**：`agent_run_lineages.relation_kind` 现在还带着 `CHECK (relation_kind = 'fork')`，是尚未清理的 closed-constraint 遗留。Channel 的等价字段（owner_kind / medium / topology）**不应该**加数据库层 CHECK 约束，只在应用层校验——这是 `MailboxSourceIdentity` 已经用行动证明过的方向。
- **长期方向**：design.md 的候选核心模型已经把 `ChannelOwner` 设计成通用枚举（`Project/Story/AgentTeam/Agent/AgentRun/ExternalBinding/System`），说明长期方向本来就不是"lifecycle 专属实体"。如果 v1 表/类型叫 `LifecycleChannel`/`lifecycle_channels`，扩展到 Project/Story 时要么迁移改名，要么产生两套并存的 channel 存储。
- **建议**：v1 直接用通用命名——类型 `Channel`（不加 `Lifecycle` 前缀，owner 是不是 lifecycle-scoped 由 `ChannelOwner::AgentRun{run_id, agent_id}` 这个 variant 表达，不需要在类型名里重复），表名 `channels`，`owner_kind`/`owner_ref` 两列做 owner 判别，不加 CHECK 约束。这样 v1 只填充 `owner_kind='agent_run'` 一种取值，后续 Project/Story 只是新增取值，不需要迁移。这个建议和 D5 是同一张表设计的两个层面，应该一起定（见下）。

### D4：`ChannelAddress` 与 `MailboxSourceIdentity` 的关系

- **先例**：当年 `MailboxSourceIdentity` 从 closed enum "重建"时，采用的是**整体替换**，不是"新建通用类型、旧结构包一层"（见 `W0-source-identity-impact.md` 的 Suggested Execution Order：先定形态，一次迁移到位，不是先加一层间接）。这次决策应该参考同一个改造风格，而不是默认选"两个类型 + 映射"这种更重的方案。
- **关键技术细节**：`MailboxSourceIdentity::dedup_fragment()`（`namespace:kind` 拼接）是完全通用的，可以原样提升成 `ChannelAddress` 的方法。但 `MailboxSourceIdentity::new()` 硬编码了 `display_label_key: format!("mailbox.source.{namespace}.{kind}")`（mod.rs:68）——这是唯一真正 mailbox 专属的部分。如果不处理，直接把整个结构提升为通用 `ChannelAddress`，会让非 mailbox 场景（例如未来 Terminal 或 Companion 直接用 channel address、不经过 mailbox）也带着 "mailbox.source." 前缀，语义就错了。
- **三个候选方案**：
  1. **整体重定位（推荐）**：把结构体搬到共享位置（例如 `agentdash-domain::channel::ChannelAddress`），`agent_run_mailbox` 侧用类型别名或 re-export 保持现有大量调用点不变；同时把 `display_label_key` 的生成从硬编码前缀改成参数化（调用方传前缀，或者干脆改成 `channel.address.{namespace}.{kind}` 这种通用前缀，mailbox 侧如果需要旧前缀就在投影层再包一层展示逻辑）。改动量小，且是本仓库已经验证过的改造风格。
  2. **新建 + 内部委托**：`MailboxSourceIdentity` 保留现有名字和 `"mailbox.source."` 前缀语义，内部持有一个 `ChannelAddress` 核心字段，方法委托过去。对 mailbox 现有调用点零改动，但多一层委托代码，且只有在"mailbox 确实需要与通用 Channel 不同的专属字段/行为"时才值得——目前找到的专属行为只有这一个硬编码前缀，收益不明显。
  3. **两个独立同构类型，靠约定同步**：不推荐——这正是 06-28 任务当初想避免的"什么都叫 channel 但各自维护"，容易 drift（`MailboxMessageSource` 的教训本质上就是"该统一的东西没统一"）。
- **建议**：方案 1，前提是先确认 `display_label_key` 前缀参数化不会影响现有 mailbox 前端投影（`display_label_key` 目前有没有被前端拿来做字符串匹配、而不是纯展示，需要在真正实现时查一下 `packages/app-web` 的消费点）。

### D5：Channel message 持久化第一版最小 schema

- **先例**：`AgentRunLineage`/`agent_run_lineages` 就是"最小关系事实表，不做事件日志"的现成模板——一条记录代表当前关系状态，不追加历史版本。design.md 已经决定"不做完整 ChannelMessage/Delivery log"，`AgentRunLineage` 证明这类小型事实表在本仓库是被验证过的模式，不是新发明。
- **教训**：同 D3——不要在 owner_kind/medium/topology 这类字段上加 DB CHECK 约束。
- **具体建议 schema**（直接对应 design.md 已有的 `ChannelOwner`/`ChannelMedium`/`ChannelTopology` 草案，字符串取值建议和 Rust enum variant 保持小写同名，方便以后直接序列化对齐）：
  - `channels` 表：`id, owner_kind, owner_ref, medium, topology, status, created_at, closed_at?`
  - `channel_participants` 表：`id, channel_id (fk), participant_kind, participant_ref, role?, joined_at, left_at?`
  - 不建 `channel_messages`/`channel_deliveries` 表——v1 消息事实继续留在 Gate/Mailbox/Terminal owner 里，Channel 只存"这个 channel 是谁、谁在里面"。
- **建议**：这个 schema 建议应该和 D3 一起定稿，因为 D3 是"这张表叫什么、owner 判别怎么设计"，D5 是"这张表除了 owner 判别还要有哪些列"——本质是同一次数据结构设计的两个提问角度，不建议分两次单独决策。

---

## 小结

D3/D4/D5 目前都已经有明确证据支持的具体建议（不是"不知道往哪个方向想"），但这三项本质上是一次具体的数据结构设计（v1 `channels`/`channel_participants` 表 + `ChannelAddress` 重定位方案），比 D1/D2/D5（命名边界）更适合直接进入"写一个小 design 草案 + 用户确认"的节奏，而不是继续留在预评估文档里反复讨论。建议下次对齐时把这三项当一个整体的"v1 数据结构确认"来过，而不是逐条单独讨论。

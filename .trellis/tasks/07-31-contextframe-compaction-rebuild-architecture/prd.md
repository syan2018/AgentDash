# 收敛 ContextFrame 编译与压缩重建架构

## Goal

让 Dash Complete Agent 以一个内聚编译器维护 ContextFrame：正常更新按Agent语义聚合来源，
压缩成功时则把 active system context 视为一次真正的 reset，从当前已接纳的 Initial Context
与 Agent Surface 重新编译一组全新的完整 ContextFrame，再加入本次 Compaction Summary。
下一轮 provider、Context inspector 与 canonical timeline消费同一组 Frame 事实。

会话消息、ToolCall 与 ToolResult 始终是 canonical conversation record。压缩后的历史前缀由
Compaction Summary 映射，保留后缀仍以原生 message 进入 provider；两者都不转换为 ContextFrame。

## First-principles Boundary

| 信息 | 权威事实 | 进入 Agent 的方式 | Canonical 展示 |
| --- | --- | --- | --- |
| 当前 identity、guidelines、environment、assignment、memory、capability、tool schema 等系统状态 | Complete Agent 已接纳的 Initial Context / Surface | ContextFrame | `ContextFrameChanged` |
| 被压缩的历史前缀 | Compaction accepted fact | Compaction Summary ContextFrame | `ContextFrameChanged` |
| 保留的用户/助手消息、ToolCall、ToolResult | Complete Agent conversation history | provider native messages | canonical conversation record |
| 历史 Surface transition | Complete Agent history | 只在reset前后对应的active context内作为增量 | 当时的 `ContextFrameChanged` |
| 当前 callable tool 机器合同 | 当前 accepted Surface tools | provider structured `tools[]` | ContextFrame 中保留同源可读说明与审计结构 |

这条边界使 ContextFrame 只承担“系统级 Agent 上下文投递事实”，不会变成会话 record 的第二套包装。

## Confirmed Problem

当前 `accepted_surface_append_frames()` 扫描全部 `SurfaceApplied` 历史并累计
`SystemAppend` Frame。压缩后 `materialize_session_context_from_state()` 仍把这条旧 delta
ledger 与当前 stable Frame、Initial Context、Compaction Summary 拼回 active recipe。

`CompactionApplied` 的 canonical projector 又从整份 history 重新物化并发布这些 Frame，
所以界面显示的是“旧 Frame 回放”，而不是压缩发生时 Agent 真正接受的一组新 context facts。
前端聚合本身没有制造重复；错误来自 Complete Agent 没有把 reset 后的 Frame 基线保存为事实。

正常更新也存在同源问题：Product把AgentFrame context fragments逐条变成Surface instruction，
Native materializer再逐条生成Frame，导致Frame数量等于来源数量；多个Identity或Guidelines来源
因而显示成多个同kind Frame。除Identity外，其他stable presentation还被统一装进
`AssignmentContext` section，结构化展示与真实语义不一致。

main-reference证明了正确粒度应是“一个Agent语义一个Frame，多个来源作为内部
fragments/sections”。但其`ContextDeliveryPlan`、cache/model-channel/connector-profile等
编排属于已删除的RuntimeSession/Connector架构，不能恢复。

## Requirements

- R1：只有成功的 `CompactionApplied + CompactionCompleted` 构成 context reset；失败、取消或 lost
  不改变 active Frame 基线。
- R2：reset 必须从压缩时的当前 accepted Agent state 重新物化，不能扫描压缩前的
  `SystemAppend` ledger 恢复当前状态。
- R3：Initial Context与Surface必须作为同一份accepted system state参与编译；stable语义按kind
  聚合，Identity、UserContext、Environment、SystemGuidelines、AssignmentContext、
  MemoryContext各自最多产生一个active Frame。
- R4：同类上游contribution必须按原始order保留为Frame内部fragments；Frame的sections与
  `rendered_text`从同一编译输入派生，不能逐contribution建Frame或把非Assignment内容伪装为
  `AssignmentContext` section。
- R5：Capability、Tool Schema、Skill、MCP、VFS、Memory、Companion等继续在一次
  `CapabilityStateDelta` Frame内表达；正常更新使用`previous → current`，重建使用
  `empty → current`。
- R6：本次 Compaction Summary 是 reset Frame 集合中的历史映射；conversation prefix不再进入
  provider，retained suffix继续保持 native messages。
- R7：正常 Surface 更新仍保持真实语义：stable Frame表达聚合后的当前值，
  `CapabilityStateDelta`表达相邻accepted Surface的真实delta。
- R8：压缩后的 active Frame fold从本次 reset Frame集合开始，只归约其后的
  `SurfaceApplied/SurfaceRevoked`；未来再次压缩时建立下一组新基线。
- R9：`InitialContextInstalled`、`SurfaceApplied/SurfaceRevoked`与`CompactionApplied`直接保存
  各次Agent实际接受的有序Frames；Frames从`InitialContextInstallation`/`DashSurface`来源对象
  中移出，不再由Native adapter预填或嵌套在当前状态对象里。
- R10：`CompactionApplied`直接保存本次完整重建的有序`Vec<ContextFrame>`，不新增
  Context snapshot、generation、ledger或独立数据库 owner。
- R11：reset Frame拥有本次 compaction occurrence 的新 ID，并统一标记
  `AppliedToCompactedContext`；其 sections、`rendered_text`与 provenance来自同一物化过程。
- R12：provider、Context inspector、usage analysis与 compaction canonical projection在同一
  history head消费同一个 active Frame fold和同一个排序规则。
- R13：normal/update/append/reset的归约语义由History payload类型与Frame kind决定，不再依赖
  connector consumption metadata或扫描Frame ID字符串。
- R14：正常 `SurfaceApplied` projector仍只发布真实 change；`CompactionApplied` projector只发布
  payload内的新 reset Frames，不重新扫描旧 history。
- R15：前端根据 Frame现有 delivery status把该批事实呈现为“上下文已重建”，并完整展示每个
  Frame；不隐藏、去重或推测后端状态。
- R16：structured `tools[]`继续来自当前 accepted Surface，ContextFrame继续承载同源的模型可读
  工具说明；conversation record与工具机器合同都不并入 ContextFrame。
- R17：删除已无生产消费者的旧RuntimeSession delivery planner协议，包括
  `ContextDeliveryPlan/Entry/Target`及cache/model-channel/connector-profile等伪执行语义；
  同时删除固定值top-level source与未发生的delivery statuses；ContextFrame只保留当前
  Complete Agent确实执行或展示的事实。
- R18：audit-only、ignore或Surface revoke提示不得伪装成Agent已消费的ContextFrame；Frame只表示
  真正进入Agent system context的内容，其他生命周期事实使用其原生canonical event。
- R19：预研数据通过一次 forward migration硬切到新格式；迁移后不存在旧 payload双读、
  runtime fallback或指向已删除 Dash source 的 Product binding。

## Acceptance Criteria

- [ ] AC1：`S1 → S2 → compact` 后，active Frames是从当前 `S2` 完整重建的新 Frame集合加本次
  Compaction Summary；`S1`初始 capability Frame与`S1 → S2` delta均不再进入 provider。
- [ ] AC2：同一个accepted state中，每个stable kind最多一个active Frame；两个Identity、
  Guidelines或Environment contribution按原始顺序显示为同一Frame内的多个fragments。
- [ ] AC3：`compact → S3` 后，stable Surface Frame按当前值替换，新增的 capability Frame只表达
  `S2 → S3`；下一次压缩前不丢失真实正常更新。
- [ ] AC4：压缩前后的当前系统状态在 sections与`rendered_text`上语义等价，但 reset Frames拥有
  新 occurrence ID与`AppliedToCompactedContext`状态。
- [ ] AC5：Compaction Summary准确覆盖被移除的 conversation prefix；retained user/assistant/
  ToolCall/ToolResult仍是原生 recipe messages和canonical records，未出现 conversation ContextFrame。
- [ ] AC6：同一 history head的 provider capture、Context inspector与 compaction presentation
  对 reset Frame IDs、成员、顺序、正文和结构完全一致。
- [ ] AC7：工具删除或 schema变更后，active Frame与 structured `tools[]`都不包含旧工具状态；
  `empty → current`重建能生成完整现状。
- [ ] AC8：Surface revoke会用Initial-only stable Frames替换整个stable slot集合，并以一个
  current-to-empty capability Frame表达移除；最新有效Compaction Summary按occurrence语义保留。
- [ ] AC9：第二次成功压缩只激活第二批 reset Frames；第一批与两次压缩之间的 delta只留在
  Agent history/canonical audit。
- [ ] AC10：manual、automatic overflow、reopen、restart与fork均从持久化 accepted facts得到相同
  active recipe；failed/lost/cancelled compaction不改变结果。
- [ ] AC11：usage只统计当前 active Frames、current tools和retained messages，不统计已被reset
  淘汰的 Frame或conversation prefix。
- [ ] AC12：前端正常更新显示“上下文已更新”，reset批次显示“上下文已重建”，且没有按 kind、
  revision或文本隐藏 Frame的逻辑。
- [ ] AC13：ContextFrame协议与前端不再包含没有生产执行者的DeliveryPlan、cache policy、
  model channel、connector profile或parser fallback；Frame顺序以accepted Vec为准。
- [ ] AC14：Surface revoke等audit/lifecycle事实不会产生`Ignore`或`AuditOnly` ContextFrame。
- [ ] AC15：迁移测试证明旧 Dash source数据被一致收敛，Product binding与Complete Agent source
  owner invariant成立，并可重新 provisioning。

## Out of Scope

- 改造 canonical conversation record、provider message或ToolCall/ToolResult pairing模型。
- 新建通用 Context Snapshot、Context Generation、Context Ledger或独立 context repository。
- 恢复main-reference的RuntimeSession、TurnPreparer、Connector或ContextDeliveryPlan。
- 接管 Codex/Remote Complete Agent无法提供的provider-private上下文。
- 重做 compaction activity、command availability或前端交互门禁。

## Locked Decisions

- ContextFrame只表达系统层级的Agent上下文投递，不包装conversation。
- Compaction成功事实本身就是reset边界，不另建generation实体。
- 正常更新与压缩重建复用同一Frame renderer；区别仅是`previous=current predecessor`还是
  `previous=empty`。
- 一个stable Frame代表一个Agent语义槽；upstream contribution只作为内部fragment。
- Frame compiler由Dash Complete Agent拥有；Native integration不拥有accepted Frame编排。
- History payload承担replace/append/reset边界；ContextFrame不再携带旧connector planner状态。
- 重建结果作为`CompactionApplied`的一部分持久化并原样投影；presentation不承担纠错。

# Session / Tool main 等价差异矩阵

参考仓库固定为 `D:/Projects/AgentDash-main-reference`，commit `957fa9d60ea3d67efa1bb278fe5b376cf0c34598`。允许差异仅限 Agent Runtime 外层 carrier/wrapper；wrapper 内 `BackboneEvent` 的内容、顺序、optional/null 语义与前端归并行为必须一致。

## 总结

当前前端 `packages/app-web/src/features/session` 并没有另一套 `AgentRuntimeFeed`：相对 main-reference 只剩 6 个 nullability/注释级差异，`useSessionFeed` 无差异，reducer 的业务结构也未重写。当前卡片分裂与会话断链的根因位于后端 producer ownership、identity、operation lifecycle、driver acceptance 和 tool invocation context。

旧 task 的架构设计本身写明“driver receipt 只代表 delivery/acceptance，不代表 business terminal”“一个 RuntimeCommandId 一个 acceptance/terminal”“brokered tool 保留完整坐标”。实现却将 receipt 放到完整 run 之后，遗漏 Native operation terminal，并让 ToolBroker 与 Native 同时发布。问题是工作流按 workstream 局部验收并过早勾选完成，缺少跨 Runtime/DB/API/reducer 的 main 行为 oracle。

## 2026-07-15 生产装配失败数据库反例

开发环境 PostgreSQL 中 `thread-59644f0c-b708-4d29-a237-d8ee1c1a0b57-d6517f27-a45a-4d9e-aa9b-dd7688b3abfe` 对应用户报告的 12:59 工具断链。它提供了本任务必须重放并消除的真实反例：

| 证据 | 持久状态 | 所证明的合同缺口 |
|---|---|---|
| 用户第二轮 | event `25` 只有一条 `user_input_submitted` | application user producer本身正常，重复/错型发生在 Native 内部 item mapper |
| Vendor presentation | event `29..31` 创建 `turn_001:tool_001..003` | Native已为三个逻辑调用分配main风格卡片身份 |
| Broker presentation | event `33/35/37` 再创建三条 `native-runtime-tool-*` | 同一逻辑调用存在第二个producer，start天然无法与Vendor result归并 |
| 三个真实工具 | `workspace_module_list=failed`、`task_read=failed`、`mounts_list=completed` | schema可见性不等于逐调用业务上下文正确；bootstrap anchor只破坏依赖frame/hook的provider |
| Vendor completion | event `52/55/58` 完成 `turn_001:tool_001..003` | 正确result存在，但前端同时收到另一套未对齐身份 |
| Native内部消息 | event `53..60` 为tool result创建通用item start/terminal | ToolResult被当作普通消息再次推进revision，污染driver item状态机 |
| 断链 | event `61 protocol_violation`，`62 turn_terminal` | 工具轮完成后没有继续到final assistant |
| operation污染 | operation `1`和`2`最终都为`lost`，错误为旧item重复started | 后一轮driver错误反向覆盖已经完成的前一轮operation |
| 重投 | 第二个outbox `attempt_count=2` | 已被Core接受并执行工具的TurnStart仍被整体重投 |

验收组合必须至少复现同一形状：一条user、同一响应多个tool、成功与业务失败并存、每个tool恰好一组start/completed、结果回灌后继续final assistant、两个operation各自独立终结且outbox副作用次数为一。

## 修复前跨层矩阵

| # | 层级 / 语义 | main-reference 行为 | 当前行为 | 证据与判定 | 修复归属 |
|---|---|---|---|---|---|
| 1 | User submission | application/session producer 发布 user input；Pi stream mapper 不把 User message 当 assistant | application presentation 正确发布后，Native internal mapper又创建 `AgentMessage` | DB seq `14..15`、`27..28`；`driver.rs` 的 `MessageStart/End` 未按 role 过滤 | Native canonical event mapper |
| 2 | Agent text | assistant delta + durable AgentMessage terminal 使用同一 stable item ID | presentation mapper基本有 main oracle；internal item还与 user/tool-result 共用通用计数器 | DB seq `17..20`；isolated fixture通过但组合语义错误 | Native mapper + parity E2E |
| 3 | Reasoning | delta、summary、terminal按 main mapper身份与顺序输出 | isolated mapper已覆盖，尚无 composition/DB/reducer组合验证 | `main-oracle-presentation.json` 只测孤立场景 | parity harness |
| 4 | Tool start/update/terminal | 旧 Pi connector是唯一 producer；同一 `turn_001:tool_00N`贯穿生命周期 | Native vendor与ToolBroker双 producer；Native内部还重复 start | DB seq `29..58` | Surface/driver emitter route + Native/ToolBroker |
| 5 | Tool card identity | reducer按 `item.id` 归并，start/result自然落同一卡片 | `turn_001:tool_00N` 与 `native-runtime-tool-*`无法合并 | `sessionStreamReducer.ts` 保持 main 行为；数据库两套 ID | 后端 identity，不改 reducer |
| 6 | Tool wrapper turn/trace | 一条 session turn identity贯穿 wrapper与 inner payload | ToolBroker coordinate `source_turn_id=None`；API wrapper可能无 turn；Native source turn、runtime turn、presentation turn又未明确映射 | `tool_broker.rs` coordinate 与 API projection | RuntimePresentationCoordinate / API projection |
| 7 | Tool result 回灌 | Agent Core消费 tool result后继续 provider loop，presentation只观察 | Broker执行结果成功回给 Core，但 mapper把 ToolResult message写成 AgentMessage；后续 projector error触发 abort | DB seq `53..60` 与日志 `Agent run aborted` | Native mapper / error boundary |
| 8 | 展示参数容错 | typed UI字段缺失时用 main 缺省或 dynamic fallback；执行层验证真实参数 | presentation projector强校验 required string并返回 critical driver violation | `fs_glob pattern`实际错误；main `unwrap_or_default` | protocol projection module |
| 9 | Driver acceptance | prompt被 Core接受后即形成幂等 delivery receipt；terminal异步收敛 | receipt在整个 run结束后才写；已执行工具仍可能返回 dispatch error | `driver.rs::dispatch/run_turn`、outbox attempt `2` | Integration Driver contract / Native driver |
| 10 | Outbox retry | 只重试未被 driver 接受的命令；接受后走 terminal/recovery | 所有普通 Host错误统一 release；同一 TurnStart整体重投 | `classify_outbox_dispatch_error`、DB seq `61..65` | outbox classification + durable acceptance |
| 11 | Operation terminal | 一个 command一个 operation terminal；turn完成后无 active operation | Native只发 TurnTerminal；第一轮 operation悬挂并被后续故障拖成 Lost | DB op sequence `1/2` | Native driver / Runtime operation correlation |
| 12 | Tool catalog | main每轮通过真实 ExecutionContext assemble工具，schema与executable共享真实 scope | provision阶段能够编译16个 schema，但 executable捕获bootstrap context | surface snapshot 与 `context_sources.rs` | Business Surface compiler |
| 13 | Task scope | active turn context含 HookRuntime；Task解析真实 session execution anchor | `hook_runtime=None` 冻结进 Task tools | `task_read`实际 missing hook runtime | invocation context resolver |
| 14 | Workspace Module anchor | 工具用真实 runtime session查询 visibility/current AgentFrame | 捕获 `surface-bootstrap-<frame>` 作为 runtime thread | `workspace_module_list`实际 missing anchor | invocation context resolver / workspace bridge |
| 15 | VFS / shell coordinates | 每轮真实 session/turn、VFS与backend anchor | VFS surface本身存在，部分工具可用；turn/session ownership仍是bootstrap值 | `mounts_list`可用不代表完整接线 | invocation context resolver |
| 16 | Hook / companion / wait | main assembled tools共享 active `hook_runtime`、pending actions与turn provenance | 同一批冻结工具均可能缺 Hook scope或携带假 turn | provider构造方式相同；需逐工具真实测试 | invocation context resolver |
| 17 | Cancel / abort | 用户 cancel或真实 provider abort才产生相应终态 | mapper错误调用 `agent.abort()`，日志表现为 provider aborted/cancelled | `run_turn` error path与实际日志 | post-acceptance error boundary |
| 18 | ContextFrame | main session reducer消费 platform context frame；当前 producer已恢复多类 frame | 需要确认工具/turn修复不改变现有 frame事件顺序与内容 | 前一任务只测 frame路径，未与工具组合 | parity E2E保护 |
| 19 | Compaction / rewind / error | main mapper有明确 compaction、error、terminal与rewind序列 | isolated mapper有 fixture；Runtime terminal projector另加 wrapper/effect，尚未做全流严格比较 | fixture七场景不覆盖真实连续流 | parity harness |
| 20 | Frontend reducer/card | main reducer按 inner protocol item与turn工作 | 当前 reducer几乎等同 main；后端坏事件被如实渲染 | session目录仅6处微小diff | 保持不动，仅增加回归测试 |
| 21 | Codex connector | Codex app server标准 item/tool lifecycle可直接作为 vendor stream，平台扩展通过wrapper | 需核对 effective emitter、ToolBroker callback与标准协议body是否单生产者 | 当前 task必须纳入 connector matrix | Codex adapter + surface binding |
| 22 | Remote connector | driver profile决定vendor stream或broker projection，不能双发 | enterprise fixture使用 `VendorStream`，production route需验证 | 当前 ToolContribution默认全为ToolBroker | Driver offer/surface binding |
| 23 | Tool result readable ref | main由session-scoped identity同时生成presentation item与大结果ref，后续工具/turn持续递增 | Native presentation identity与ToolResultRef address分离；fallback可反复生成`turn_001:tool_001` | 第2/3个工具或后续turn的截断结果可能回填错误card | Native shared identity context |
| 24 | Permission / VFS enforcement | AgentFrame grants投影为真实VFS policy，Broker在副作用前统一授权 | current surface构造whole-mount policy/default grant；Registry Broker permission/VFS gate恒Allowed | 工具可见不等于调用被正确授权 | Surface compile + ToolBroker policy |
| 25 | Surface closure | capability、VFS与HookPlan闭包缺失时typed provision failure | 多处`unwrap_or_default`让残缺surface继续生成binding | 错误被推迟到工具运行时才表现为missing anchor | Surface query/materialization |
| 26 | Immutable launch provenance | main执行上下文区分launch evidence、current frame与orchestration/node/attempt | current frame被写为launch evidence，orchestration/node/attempt为空 | restart/rebind后无法证明执行归属与原始launch | Binding/thread anchor persistence |
| 27 | 持久 transcript 恢复 | main从完整session event重建user、assistant tool-call与同call id的tool result，重绑后交给Agent Core | Native只把`surface.context.blocks`转成messages；Runtime snapshot又只索引`item_completed`，遗漏user和完整配对 | 进程重启/rebind后历史丢失，compaction可能得到assistant/tool-only上下文 | Runtime journal transcript broker + shared projector |
| 28 | Transcript typed item覆盖 | main恢复器覆盖command/file change/MCP/dynamic/native工具族并补齐缺失tool output | Native恢复器把DynamicToolCall单独变成伪造`restored-*` ToolResult，拒绝多种typed item | shell/fs/MCP历史可令binding恢复失败或provider收到孤立tool result | shared canonical transcript projector |
| 29 | Readable identity恢复 | main从持久session item/ref恢复`turn_NNN:tool_NNN|cmd_NNN`水位 | 当前只扫描canonical context item id；真实ID为`native-runtime-tool-*`，解析器直接忽略 | 重启后重新从`turn_001:tool_001`分配并与旧card冲突 | durable presentation transcript + Native session identity |
| 30 | Live AgentFrame展示 | main在AgentFrame/surface变更时发布capability、assignment等ContextFrame，Native能力不决定平台事实是否存在 | canonical `SurfaceAdopt`先被Native profile能力拒绝，ContextFrame尚未写journal | Native会话看不到运行期ContextFrame更新 | Runtime canonical adoption + connector-specific lowering |
| 31 | ContextFrame identity/order | main按frame family生成稳定ID，bootstrap顺序为capability→assignment→system delivery→identity→user→environment→guidelines→memory | 通用`context-frame-{operation}-{ordinal}`破坏family ID，组合顺序只被局部测试覆盖 | 前端无法把同类frame与main一致归并/展示 | context projector + combination oracle |
| 32 | 非Turn operation终态 | main每个command都有唯一acceptance/terminal；delivery-only命令在driver接收点完成 | Resume/Fork/Settings/Steer/Interrupt/Interaction/ToolSet/SurfaceAdopt可永久active | 后续故障会错误拖累已经成功的旧operation | Runtime outbox acceptance terminal |

## 为什么已有测试没有发现

1. `main-oracle-presentation.json` 只对 isolated mapper 的七组事件做 protected body比较，没有经过 ToolBroker、Runtime journal、outbox、Session API 与 reducer。
2. Native mapper测试直接调用 `ToolExecutionStart/Update/End` 并声称 vendor是唯一 emitter，却没有加载 production catalog；真实 catalog全部声明 `ToolBroker`。
3. ToolBroker测试分别验证 `ToolBroker` 与 `VendorStream` 分支，但没有和 Native mapper组合，所以两个局部测试都通过，生产却双发。
4. Runtime mailbox/outbox测试验证 command identity重用，但没有模拟“接受后已产生事件，再由 projector 返回错误”的边界。
5. E2E只覆盖单句文本或单个理想化工具，没有连续多工具、业务错误、可选参数缺失、Task/Hook、Workspace Module、第二轮继续、重启与真实数据库状态。
6. 原 task workstream 的 acceptance checkbox由各层局部测试勾选，`implement.jsonl/check.jsonl`主要列规范文档，没有建立跨 workstream 的行为矩阵和数据库 oracle。
7. catalog测试只证明16个名称/schema能够materialize，部分调用还丢弃`tool.execute()`返回值；因此bootstrap context、恒Allowed policy和残缺closure都未进入断言。
8. 所有现有恢复测试都直接喂给mapper理想化`ContextBlock::RuntimeItem`或readable ID，没有从真实PostgreSQL journal重建user/tool-call/tool-result，也没有销毁binding后继续下一轮。

## 目标 connector / presentation ownership 矩阵

| Connector | 工具执行 | 标准/扩展展示 producer | ToolBroker职责 |
|---|---|---|---|
| Native Agent Core | Platform ToolBroker direct callback | Native vendor stream，严格复用 pinned main mapper | policy、execution、internal canonical tool state；不重复发布presentation |
| Codex App Server | 标准 dynamic tool/MCP/command bridge或Platform callback | Codex标准 item notification优先作为 vendor stream；AgentDash扩展用同一wrapper承载 | 只执行平台工具与保持internal state；只有无vendor lifecycle的明确route才投影 |
| Enterprise Remote | 按 offer/profile选择callback/MCP | 由声明的真实 vendor stream能力决定 | fallback route必须显式且逐binding唯一 |

`ToolPresentationEmitter` 不能只作为全局 ToolContribution 默认值后在 `DriverToolDefinition` 中丢失。它必须在 binding/profile 求交后形成 effective route，并同时约束 driver mapper与ToolBroker，确保 exactly-one producer。

## 完成判据

差异矩阵中所有 WRONG/MISSING/PARTIAL 项必须由 production code、数据库断言和 main protected body oracle共同关闭。仅让一个日志消失、一个 mapper test通过或一个 card看起来合并，不构成完成。

## 最终关闭矩阵（2026-07-15）

以下状态以本文件开头固定的 main-reference、真实 PostgreSQL/production composition 和现有前端 reducer 为共同判据；`CLOSED` 表示修复前差异已消除，不表示把新 Runtime 的内部 carrier 改回 main 结构。

| # | 最终状态 | 关闭后的行为 | 主要证据 |
|---|---|---|---|
| 1 | CLOSED | User submission 只由 application producer 发布，Native 不再把 User 映射为 AgentMessage | Native 全包 51 tests；前端 parity user exact-once |
| 2 | CLOSED | Assistant delta 与 durable terminal 共享 presentation item，最终文本连续 | Native `native_message_start_has_no_phantom_presentation_and_terminals_are_independent`；Remote production E2E |
| 3 | CLOSED | Reasoning live delta 与 durable reasoning terminal 均进入 Session 事件链 | Remote production E2E 同时捕获 transient live 与 cold replay terminal；前端组合 parity |
| 4 | CLOSED | effective route 约束 Native vendor stream / Broker 单一 producer，每个工具仅一套 lifecycle | Native `native_vendor_tool_stream_is_the_single_complete_presentation_emitter`；真实六工具 E2E |
| 5 | CLOSED | start/update/completed 以同一 protected `item.id` 归并为一张卡 | production E2E journal lifecycle；前端 reducer/card 586 tests |
| 6 | CLOSED | runtime/source/presentation turn 与 item/call 坐标分层保存，wrapper 只承载坐标 | contract schema/check；Remote descriptor 与 Session wrapper 断言 |
| 7 | CLOSED | ToolResult 只回灌 Core，不创建通用 AgentMessage；工具轮继续到 final assistant | Native 三工具 round-trip；六 provider cold rebind E2E |
| 8 | CLOSED | partial/null typed presentation 使用 main 等价 dynamic fallback，执行层仍负责真实参数校验 | `native_partial_fs_glob_arguments_fall_back_without_aborting`；catalog `fs_glob` 门禁 |
| 9 | CLOSED | Core 接受 prompt 即登记 receipt，provider/tool loop 在独立 event pump 收敛 | Native driver failure/acceptance tests；outbox attempt_count 断言 |
| 10 | CLOSED | 未接受命令才重试；同 thread 前序 SurfaceAdopt 失败会阻断后续 TurnStart | PostgreSQL `runtime_outbox_preserves_thread_causality_across_surface_adopt_retry` |
| 11 | CLOSED | owning/source operation 各自 exact-once terminal，后续 BindingLost 不改写历史成功 | Runtime 43 tests；Native/Remote production operation 断言 |
| 12 | CLOSED | catalog definition 与 executable owner 均来自 production AgentFrame composition | 六类 provider catalog 3 tests；production Native 六工具 E2E |
| 13 | CLOSED | Task 每次调用解析真实 Hook Runtime 与 frame scope | production `task_read` 执行及 cold rebind E2E |
| 14 | CLOSED | Workspace Module visibility 使用 canonical binding/thread/frame anchor | production `workspace_module_list` 执行及 typed owner scope test |
| 15 | CLOSED | VFS/mount 使用真实 session/turn/backend 坐标并经过统一 policy | 六 provider E2E；ToolBroker VFS deny-before-side-effect |
| 16 | CLOSED | Workflow/Collaboration/Task/Wait/Workspace Module 共享逐调用 typed context | `all_six_production_providers_execute_with_real_typed_owner_scope` |
| 17 | CLOSED | mapper/sink 故障不再伪造成 provider abort；真实 interrupt/lost 各自产生唯一终态 | Native interrupt/sink tests；Remote active disconnect E2E |
| 18 | CLOSED | bootstrap 与 live AgentFrame 重新发布全部 ContextFrame family | Runtime context projection tests；Native SurfaceAdopt canonical frame test |
| 19 | CLOSED | compaction 使用冻结 source boundary；error/lost 产生 terminal + rewind | compaction 15 tests；Remote compaction/recovery/disconnect E2E |
| 20 | CLOSED | `features/session` 保留 main UI：105 个文件中 99 字节一致、6 个明确 seam | `sessionParityEvidence`；app-web 93 files / 586 tests |
| 21 | CLOSED | Codex 0.144.1 标准 body/null 语义保留，dynamic callback 走 Broker 且 sink 成功后才 pending | Codex 48 tests；main fixture deep-equal |
| 22 | CLOSED | Remote offer/profile 决定 effective producer，generation fence 与 transcript HostPort 可恢复 | Remote 18 tests；enterprise RuntimeWire E2E |
| 23 | CLOSED | session-scoped identity 同时驱动 card 与 large-result ref，跨 turn/rebind 单调递增 | Native identity watermark tests；production cold rebind E2E |
| 24 | CLOSED | permission/VFS gate 在 owner executor 副作用前执行，grant 来自 AgentFrame | ToolBroker 15 tests；六 provider typed scope test |
| 25 | CLOSED | capability/VFS/HookPlan closure 缺失时 typed provision failure，不产生可调用 binding | business surface 20 tests；production surface validation |
| 26 | CLOSED | launch provenance、current frame、orchestration/node/attempt 与 binding generation 分离并可恢复 | AgentFrame recovery/context broker PG tests；cold rebind E2E |
| 27 | CLOSED | durable transcript broker 重建 user、assistant、paired call/result 与 compaction tail | shared transcript projector 27 protocol tests；Native/Remote cold recovery |
| 28 | CLOSED | command/file/MCP/dynamic/native item 经同一 projector 恢复，不伪造孤立 ToolResult | transcript projector tests；Native cold-start integration tests |
| 29 | CLOSED | readable ID 水位从 durable presentation 恢复，新卡严格高于旧卡 | `restored_tool_item_ids_advance_session_identity_watermarks`；production cold rebind E2E |
| 30 | CLOSED | canonical SurfaceAdopt 先提交平台 frame；Codex full adopt，Native 仅按真实 profile 下沉 ToolSetReplace | Runtime `native_surface_adopt_commits_context_frames_and_lowers_driver_sync_to_tool_replace`；Codex deferred adopt test |
| 31 | CLOSED | ContextFrame 使用 main family ID、顺序与 payload，组合 oracle 覆盖 bootstrap/live | context projection 23 unit tests 与 main fixture |
| 32 | CLOSED | 非 Turn command 在 acceptance boundary 终结；active SurfaceAdopt 按同 thread 因果队列串行 | Runtime host/operation tests；PostgreSQL outbox causality test |

Native 的 full surface hot adoption 仍由 profile 明确声明为 unsupported；这不是平台事件缺失或兼容回退。平台 canonical AgentFrame/ContextFrame 已完整提交，Native 当前只同步其实际承载的 ToolSet 子集，为后续原生能力保留明确接入空间。

## 最终质量记录

- 通过：Managed Runtime 全包、Native/Codex/Remote 全包、Agent protocol、六类 production provider、Native cold rebind、Remote enterprise RuntimeWire/compaction/rebind/disconnect、contracts generate/check、migration guard、app-web typecheck、app-web 93 files / 586 tests、本次前端文件 lint、Rust fmt 与 diff check。
- 全仓 app-web lint 仍被 33 个未修改文件中的既有 React Compiler 规则错误阻断；本任务修改文件 lint 为零错误。
- 全仓严格 clippy 首先命中 12 类可在任务前 HEAD 原样复现的既有警告；仅在命令行豁免这些基线类别后，六个关键改动包的 `--all-targets -D warnings` 通过，并修复了本任务新引入的三条警告。
- test-support boundary guard 仍被未修改的 `control_effects.rs::MemoryControlEffectStore` 与 `fork_command.rs::FakeGraphStore` 基线命名/归属阻断；migration guard 通过。

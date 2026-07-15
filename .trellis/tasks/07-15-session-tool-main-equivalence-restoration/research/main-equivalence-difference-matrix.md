# Session / Tool main 等价差异矩阵

参考仓库固定为 `D:/Projects/AgentDash-main-reference`，commit `957fa9d60ea3d67efa1bb278fe5b376cf0c34598`。允许差异仅限 Agent Runtime 外层 carrier/wrapper；wrapper 内 `BackboneEvent` 的内容、顺序、optional/null 语义与前端归并行为必须一致。

## 总结

当前前端 `packages/app-web/src/features/session` 并没有另一套 `AgentRuntimeFeed`：相对 main-reference 只剩 6 个 nullability/注释级差异，`useSessionFeed` 无差异，reducer 的业务结构也未重写。当前卡片分裂与会话断链的根因位于后端 producer ownership、identity、operation lifecycle、driver acceptance 和 tool invocation context。

旧 task 的架构设计本身写明“driver receipt 只代表 delivery/acceptance，不代表 business terminal”“一个 RuntimeCommandId 一个 acceptance/terminal”“brokered tool 保留完整坐标”。实现却将 receipt 放到完整 run 之后，遗漏 Native operation terminal，并让 ToolBroker 与 Native 同时发布。问题是工作流按 workstream 局部验收并过早勾选完成，缺少跨 Runtime/DB/API/reducer 的 main 行为 oracle。

## 跨层矩阵

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

## 为什么已有测试没有发现

1. `main-oracle-presentation.json` 只对 isolated mapper 的七组事件做 protected body比较，没有经过 ToolBroker、Runtime journal、outbox、Session API 与 reducer。
2. Native mapper测试直接调用 `ToolExecutionStart/Update/End` 并声称 vendor是唯一 emitter，却没有加载 production catalog；真实 catalog全部声明 `ToolBroker`。
3. ToolBroker测试分别验证 `ToolBroker` 与 `VendorStream` 分支，但没有和 Native mapper组合，所以两个局部测试都通过，生产却双发。
4. Runtime mailbox/outbox测试验证 command identity重用，但没有模拟“接受后已产生事件，再由 projector 返回错误”的边界。
5. E2E只覆盖单句文本或单个理想化工具，没有连续多工具、业务错误、可选参数缺失、Task/Hook、Workspace Module、第二轮继续、重启与真实数据库状态。
6. 原 task workstream 的 acceptance checkbox由各层局部测试勾选，`implement.jsonl/check.jsonl`主要列规范文档，没有建立跨 workstream 的行为矩阵和数据库 oracle。
7. catalog测试只证明16个名称/schema能够materialize，部分调用还丢弃`tool.execute()`返回值；因此bootstrap context、恒Allowed policy和残缺closure都未进入断言。

## 目标 connector / presentation ownership 矩阵

| Connector | 工具执行 | 标准/扩展展示 producer | ToolBroker职责 |
|---|---|---|---|
| Native Agent Core | Platform ToolBroker direct callback | Native vendor stream，严格复用 pinned main mapper | policy、execution、internal canonical tool state；不重复发布presentation |
| Codex App Server | 标准 dynamic tool/MCP/command bridge或Platform callback | Codex标准 item notification优先作为 vendor stream；AgentDash扩展用同一wrapper承载 | 只执行平台工具与保持internal state；只有无vendor lifecycle的明确route才投影 |
| Enterprise Remote | 按 offer/profile选择callback/MCP | 由声明的真实 vendor stream能力决定 | fallback route必须显式且逐binding唯一 |

`ToolPresentationEmitter` 不能只作为全局 ToolContribution 默认值后在 `DriverToolDefinition` 中丢失。它必须在 binding/profile 求交后形成 effective route，并同时约束 driver mapper与ToolBroker，确保 exactly-one producer。

## 完成判据

差异矩阵中所有 WRONG/MISSING/PARTIAL 项必须由 production code、数据库断言和 main protected body oracle共同关闭。仅让一个日志消失、一个 mapper test通过或一个 card看起来合并，不构成完成。

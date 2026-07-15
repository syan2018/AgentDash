# 恢复 Session 消息与工具调用 main 等价链路

## Goal

除 Agent Runtime 内部结构与边界解耦外，完整恢复 `main-reference` 的 Session 可观察行为。以实际 PostgreSQL journal / projection、main-reference 生产代码和前端 reducer/card 为共同 oracle，修复工具调用生命周期、消息连续性、取消传播、runtime surface anchor 与 Hook scope 接线，禁止再以单轮理想化 fixture 证明完成。

## Background

当前分支已经出现可复现的跨层断链：

- 同一工具调用的 start 与 completed/result 没有归并为同一前端 card；
- 工具调用结束后 Agent loop 不继续，日志出现 `Agent run aborted`；
- `fs_glob` 的 typed projector 因参数缺少 `pattern` 将整个 driver event 判为 protocol violation；
- 工具执行读取不到 `workspace_module_visibility` runtime surface anchor；
- 工具执行读取不到 Hook Runtime，无法定位 Task scope；
- 此前测试主要覆盖一句输入、一个理想化工具和一个终态，没有覆盖连续工具链、错误工具、真实 workspace/Hook surface、取消与多轮继续。

参考仓库固定为 `D:/Projects/AgentDash-main-reference`。数据库中实际 Runtime journal、ToolBroker call、Session projection 和 mailbox/outbox 状态必须参与证据链，不能只比较类型或 mapper 单测。

## Requirements

### R1. 全链路差异矩阵

- 从 composer 输入开始，逐层对比 main-reference 与当前分支的 mailbox、Runtime command/outbox、Native Agent、ToolBroker、canonical journal、Session API projection、frontend reducer 与 card identity。
- 对每一处差异记录 main 行为、当前行为、数据库证据、根因层级、修复归属和回归测试。
- 审计至少覆盖 user/agent/reasoning/tool start/tool update/tool completed/tool result/terminal/error/rewind/compaction/context frame。

### R2. 工具 Card identity 与 payload 等价

- 同一逻辑工具调用的 started、updated、completed/result 必须使用 main-reference 等价的 thread/turn/item/call identity，并归并为一个前端 card。
- Backend wrapper 可以不同，wrapper 内协议内容、可选/null 语义、工具参数、状态、输出与顺序必须与 main-reference 等价。
- 工具 projector 不得因 provider 省略可选参数而破坏整个 turn；参数校验与工具执行的权威边界必须和 main-reference 一致。

### R3. 连续 Agent loop 与取消语义

- 工具成功、业务错误、拒绝、超时和可恢复 projector/展示错误后，Agent loop 的继续/终止行为必须与 main-reference 一致。
- 正常工具轮不得被错误传播为 provider `aborted`，也不得留下 active Item、active Turn、处理中 prompt 或重复 outbox dispatch。
- 至少验证“assistant tool call → tool result → provider continuation → assistant final”以及多工具、多轮 composer follow-up。

### R4. Runtime surface 与业务 anchor 完整恢复

- 工具执行所需的 workspace module visibility、VFS/mount、Task/Hook Runtime、identity、permission/capability 等业务事实必须从正确的 AgentFrame/Runtime surface 边界恢复。
- Surface compile只固化工具 definition、capability closure与不可变 owner provenance；每次 invocation从 canonical binding/thread/turn/frame解析 typed execution context，并把真实 Hook Runtime、permission grant、VFS access与workspace visibility交给 owner executor。
- launch frame、orchestration/node/attempt、binding generation与tool-set revision必须来自可恢复的不可变 anchor；current frame只表达当前上下文，不能替代 launch provenance。
- capability/VFS/HookPlan surface closure缺失或非法时以 typed provision failure终结，使 binding无法带着默认空 surface进入执行。
- 不允许工具 executor 从前端状态、临时 session DTO 或不存在的旧 RuntimeSession 字段猜测事实。
- Native、Codex 与 remote connector 必须各自把工具调用转接到同一 Platform ToolBroker 契约；Native 特有能力可以明确悬空，但不得伪装为已接通。

### R5. 数据库与重启/并发不变量

- 对实际失败 run 查询 Runtime journal、entity projection、tool call、binding、outbox、mailbox、surface artifact 与 Hook state，确认所有 identity 和状态迁移。
- 工具调用及其 presentation 在并发 driver fact、重启、rebind 和重放下保持幂等，不出现 card 分裂、行级联丢失、重复执行或终态缺失。

### R6. 行为级验证

- 建立 main-reference 驱动的固定 oracle 与数据库断言，不接受只测 mapper/serde/单句文本。
- 真实 E2E 至少覆盖：连续两个工具、工具业务错误后继续、缺省可选参数、workspace module 工具、Task/Hook scoped 工具、多轮 follow-up、主动取消、进程重启后继续。
- 前端必须验证实际 reducer 输出和 card 合并结果，不只验证 API JSON 可解析。

## Acceptance Criteria

- [ ] AC1：产出覆盖数据库到前端的完整差异矩阵，每一处非结构性差异都有源码与实际数据证据。
- [ ] AC2：同一工具调用的 start/update/completed/result 在数据库、Session API 和前端均归并到同一 identity/card。
- [ ] AC3：`fs_glob` 等工具的缺省/可选参数不会造成 driver protocol violation，行为与 main-reference 一致。
- [ ] AC4：工具成功、业务错误、拒绝与超时后 Agent loop 均按 main-reference 继续或终止，不再出现错误 `Agent run aborted`。
- [ ] AC5：workspace module visibility 与 Task/Hook Runtime anchor 在 production surface 中可用，相关真实工具不再返回 missing anchor/scope。
- [ ] AC6：连续工具链、多轮 follow-up、取消、重启/rebind 均无 active entity 悬挂、重复 dispatch 或 mailbox 堆积。
- [ ] AC7：Session eventstream 除允许的 wrapper 外，内容、顺序、null/optional 语义与 main-reference 固定 oracle 一致。
- [ ] AC8：前端 Session feed/reducer/card 行为相对 main-reference 无非预期变化，现有 UI 不被替换。
- [ ] AC9：Native、Codex、remote connector 的工具桥接边界与 Platform ToolBroker 契约明确且有实际生产装配测试。
- [ ] AC10：Rust、frontend、migration、contract、数据库集成及真实 dev E2E 全部通过，差异矩阵不存在 MISSING/PARTIAL/WRONG。
- [ ] AC11：permission/VFS deny在任何副作用前生效，非法 surface closure无法 provision，重启/rebind后 launch provenance、binding generation与真实 invocation anchor保持可恢复且一致。

## Out of Scope

- 不重设计 Session UI。
- 不以兼容层保留已确认错误的双链路或旧 RuntimeSession 事实源。
- 不改变 Codex App Server Protocol；AgentDash wrapper 仅负责承载平台扩展。
- 不把彼此独立、能够单独关闭的产品需求塞入本任务；但本次消息/工具链的跨层根因必须作为一个整体收口。

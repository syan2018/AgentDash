# Session / Tool 链路返工复盘

## 1. Root Cause Category

- **B — Cross-Layer Contract**：Runtime、Driver、ToolBroker、Session journal与frontend reducer之间没有共同固定presentation owner、identity、acceptance/terminal与cursor坐标。
- **C — Change Propagation Failure**：Runtime重构替换了内部边界，却没有同步恢复provider transcript、callable registry、AgentFrame owner context、connector terminal sink与generated wire传递依赖。
- **D — Test Coverage Gap**：局部mapper、schema catalog和单句turn都能通过，但没有装配真实PostgreSQL、真实AgentFrame/Hook/VFS/Workspace provider、多个工具、业务错误、rebind与frontend reducer。
- **E — Implicit Assumption**：实现隐含假设“过滤后的journal sequence仍连续”“tool callback期间surface不变”“driver receipt等于业务完成”“schema可见等于工具可执行”。

## 2. Why Fixes Failed

1. **只修消息mapper**：消除了某个重复item，却没有确定Driver与Broker的effective presentation route，生产装配仍可能双发。
2. **只补ToolContext字段**：schema能编译，但executable仍捕获bootstrap owner；真实Task/Workspace provider继续缺Hook/runtime anchor。
3. **只测一句输入**：未进入tool result回灌、第二次provider request、terminal sink和active turn清理，因此`aborted`与卡死无法出现。
4. **只恢复surface recipe**：重启时没有用active checkpoint覆盖driver surface与callable registry，rebind后上下文与工具版本漂移。
5. **只按可见事件重编号**：live使用raw Runtime sequence、GET/replay使用dense sequence，断线cursor跨internal gap后跳过terminal/ContextFrame。

## 3. Prevention Mechanisms

| Priority | Mechanism | Specific Action | Status |
| --- | --- | --- | --- |
| P0 | Architecture | binding固化effective projector/emitter；canonical/runtime、presentation、source identity显式分型 | DONE |
| P0 | Runtime | acceptance、business terminal、sink commit和outbox ack分别持久化并各自幂等 | DONE |
| P0 | Test | embedded PostgreSQL production composition覆盖多工具、业务错误、continuation、disconnect/rebind与真实owner provider | DONE |
| P0 | Protocol | protected `BackboneEvent` body与main一致，wrapper之外不改变内容/顺序/null语义 | DONE |
| P1 | Recovery | durable transcript、active checkpoint、callable registry与readable identity水位共同恢复 | DONE |
| P1 | Cursor | GET/live/replay/fork统一使用raw Runtime EventSequence坐标 | DONE |
| P1 | Generation | contracts generator导出RuntimeWire传递依赖闭包并由frontend typecheck守门 | DONE |
| P1 | Review | cross-layer thinking guide加入single producer、cursor gap、active surface与真实provider检查项 | DONE |

## 4. Systematic Expansion

- **Similar Issues**：Hook callback、approval interaction、remote HostPort与compaction activation都同时跨越canonical state和connector side effect，必须沿用相同的acceptance/terminal/sink-commit模型。
- **Design Improvement**：Business Surface只编译definition与immutable provenance；每次ToolBroker invocation从durable binding/turn/frame解析typed owner context。
- **Process Improvement**：涉及Session行为的Runtime重构，以固定main-reference protected body、production database与现有frontend reducer三者共同验收。

## 5. Knowledge Capture

- [x] 更新cross-layer thinking guide。
- [x] 更新Native、Codex、Context、AgentRun facade、Backbone与RuntimeWire规范。
- [x] 把差异矩阵与production E2E保留在本任务中作为可复核证据。

# Agent Runtime 持久化权威收敛结果

## 最终形态

```text
Product owner document
  LifecycleRun / LifecycleAgent / frames / workflow / lineage / Agent association
        |
        | synchronous input + stable effect identity
        v
In-memory Runtime + In-memory Complete Agent Host
  normalize / live broadcast / attach / route / callback fence
        |
        v
Concrete Complete Agent authority
  source / history / context / fork / compaction / effect receipt / applied surface
```

Runtime 和 Host 位于两个 durable owner 之间。它们能够从 Product association 与 concrete Agent
`read/inspect` 重建，因此没有独立跨重启业务寿命。一次 Agent 操作只有 concrete Agent 这一处
执行 authority；Product 只保存业务意图与定位关联，并返回 Agent 的真实 operation receipt。

## 持久化边界

| 状态 | owner | 物理形态 | 持久化理由 |
| --- | --- | --- | --- |
| Lifecycle/Frame/association | Product LifecycleAgent | `lifecycle_agents` + owner-local JSONB | Product 业务归属与执行意图 |
| Workflow/Gate/Routine/Channel | 对应 Product 聚合 | owner row/document/effect | 独立业务等待、编排与下游 evidence |
| Workspace/Terminal presentation | 对应 Product effect | 独立 Product store | 产品资源与终端副作用 |
| Dash source/history/context | Dash Complete Agent | `dash_complete_source.document` | concrete Agent native authority |
| Create 前 effect receipt | Dash Complete Agent | `dash_complete_effect` | source 产生前按 effect identity inspect |
| Tool/Hook receipt | 实际 handler owner | owner-specific effect | 外部副作用幂等 |
| Runtime/Host route/live delta | 无 durable owner | process memory | 可从 Product binding 与 Agent source 重建 |

## Command、Read 与 Live

```text
Command:
Product target + client identity
  -> stable Agent effect
  -> restore current Host route from full Product binding
  -> Complete Agent inspect/execute
  -> concrete Agent receipt

Read / reconnect:
Product shell + Complete Agent read(source)
  -> in-memory canonical conversation view
  -> authoritative UI snapshot

Live:
Agent Core callback
  -> source-scoped canonical record broadcast
  -> current UI connection
```

Complete Agent snapshot、Runtime request-scoped wrapper与前端feed共享唯一
`CanonicalConversationRecord`流；Runtime/UI不再维护平行turn/item表示。live event只承担当前连接中的
低延迟partial，重连以authoritative snapshot替换；会话运行态只由`TurnStarted/TurnCompleted`界定。
即使Provider在输出assistant item前失败，Agent terminal history也会形成terminal-only turn segment，
并在UI展示真实错误信息。

## Cold Host 与 Provider 边界

Product read、command、live subscription 与 fork snapshot 都将完整
`AgentRunProductRuntimeBinding` 交给 resolver。resolver 先恢复当前 Host route，再返回同一次解析得到的
`CompleteAgentService + AgentBindingGeneration`，因此空 catalog 不会阻断由 durable binding 驱动的
冷启动恢复。

平台 thinking level 表达稳定语义层级，Provider adapter 负责 wire encoding。OpenAI Responses 保留
`minimal`，Codex Responses 将平台最低非零档编码为其支持的 `low`；不修改 profile、source identity，
也不通过运行时 fallback 猜测能力。

## Fork 状态机

普通 Fork 继承 concrete Agent child binding，Product graph commit 后直接 Activate。只有显式选择新的
ProjectAgent/execution profile 时才物化 selected frame、Rebind 并 Activate。Saga next-step、runtime
operation acceptance 与重启 inspection 使用同一个 typed selection 条件，避免普通 Fork 额外查询或
重写 Product binding。

## Production Tracer Bullet

- 使用既有 Product binding，在全新 Host 进程中恢复 Dash service、source route 与 binding
  generation；首次 authoritative snapshot 读取成功。
- 向 AgentRun `814b65c6-633d-598a-a458-ec98f53a8641` 提交真实 Composer input，要求使用
  filesystem tools并返回`STREAM_OK`。
- API 返回 concrete Agent operation receipt `succeeded`。
- 同一页面依次渲染`mounts_list`、`fs_glob`两个工具项与Agent message
  `STREAM_OK Cargo.lock`，没有未知工具卡或首包后中止。
- authoritative snapshot重读到revision 14与14条ordered canonical records；contract只有
  `conversation_history`，没有平行turn/item/active事实。
- 浏览器重载后从Dash durable history恢复相同工具项和最终消息。
- PostgreSQL 中 `dash_complete_effect` 收敛为 terminal succeeded；`dash_complete_source` 保存
  source command/history。LifecycleAgent 只保存稳定 Product binding/profile/source，没有 Runtime、
  Host 或 live projection durable facts。

## Verification

- 相关 Application/Runtime contract/Infrastructure/API/LLM Provider crates `cargo check` 通过。
- `agentdash-application-agentrun` 134 项测试通过，包含 Fork crash-window/inspection matrix 18 项。
- `agentdash-llm-provider` 50 项测试通过。
- frontend TypeScript typecheck与99个文件、504项测试通过；live transport、presentation merge、
  turn liveness、authoritative reload、terminal-only failure与错误渲染均在其中。
- contracts 六组完整generate/check全部无漂移，RuntimeWire schema不再残留平行Agent item/turn词汇。
- migration history guard 通过；本次最终修复不新增 schema 或 migration。
- 退役 Runtime schema、repository/gate 与 Noop callback sink 的 production 源码负向搜索通过。
- 本次新增与直接修改的独立前端模块 ESLint 通过；hook 文件仍保留仓库既有
  `react-hooks/set-state-in-effect` 基线诊断，不影响本任务数据链路验收。
- `git diff --check` 通过。

# Agent Runtime Context

AgentDashboard 用 Product、Complete Agent 与进程内 Runtime 三个边界表达 AgentRun。持久化只服务于
各业务 owner 无法从别处恢复的事实；跨层读取通过 typed coordinate 访问事实 owner，不建立平行
投影或同步账本。

## Language

**AgentRun Product**:
面向用户与工作流的产品 aggregate，拥有 LifecycleRun、LifecycleAgent、owner-local AgentFrame、
Routine、Workflow、Companion、资源配置以及 concrete Agent association。Product 负责选择输入和
上下文意图，并把它们交给绑定的 Complete Agent；它不保存 Agent conversation。

**Complete Agent**:
实现完整 Agent 语义的服务边界。它拥有 source、history、context、fork、compaction、command
effect 与 terminal evidence，并通过 `create/resume/execute/read/inspect/apply_surface` 等协议提供
事实和能力。Dash、Codex 与企业 Agent 可以采用不同内部模型，但必须在此边界返回 canonical
conversation 与 typed receipt。

**Concrete Agent State**:
具体 Complete Agent 自己维护的 canonical source document。对 Dash 而言，history、surface、
context、branch、compaction 与 effect state 在同一个 JSONB document 中通过 owner-local CAS 原子
提交；当前 surface 是 history fold 的结果，不是并行仓储字段。

**Agent Runtime**:
当前进程中的事务协调与调用机制。它根据 Product association 定位 Complete Agent service/source，
执行同步 handoff、Host callback route 与 process-local live broadcast。进程重启后从 Product
association 和 Complete Agent authority 重新组合，不恢复独立 Runtime 状态。

**Complete Agent Host**:
当前进程内承载 Complete Agent attachment、verified target、binding generation 与 callback route
的执行边界。Host 事实用于调用隔离和 fence；需要恢复的业务事实分别属于 Product 与 Complete
Agent，因此 Host 不形成 durable owner graph。

**Canonical Conversation**:
Complete Agent native history 经唯一 projector 得到的 `CanonicalConversationRecord` 序列。
snapshot 与已提交 live record 共用同一 projector、presentation identity 和顺序；provider/Core
只补充当前进程内的 ephemeral delta。用户消息、助手消息、ToolCall 与 ToolResult 始终属于该
record lane；压缩只改变 active history boundary，不把它们转换为 ContextFrame。

**Agent Surface**:
Product 只提交期望的 prompt、tool 与 initial context；concrete Agent 将实际接纳的 surface/context
写入自己的 history。Surface 表达当前系统指令与工具状态，不拥有 conversation。

**ContextFrame**:
concrete Agent 已接纳并实际进入Agent system context的语义单元，也是该事实的canonical
presentation。相同语义的多个上游来源聚合为一个Frame，并在内部保留有序fragments/sections；
Frame粒度不跟随contribution粒度。正常Surface变更产生聚合后的stable Frame与真实transition
Frame；成功压缩从当前accepted state生成新的完整Frame基线，替换旧active Frame链。

**AgentFrame**:
Product owner-local 的平台业务 frame，用于描述选择、关联和 materialization intent。AgentFrame
可以参与生成 Agent surface，但不等于 concrete Agent context，也不改写 conversation history。

**Effect Receipt**:
业务 owner 对一次稳定 effect identity 的接纳或终态证明。Product 保存自己的 workflow/lineage
事实；Complete Agent 保存 public effect 与 terminal evidence。Runtime operation receipt是同一
effect identity穿过调用边界时的opaque handle，不是第二个aggregate。调用方通过该identity的
`inspect` 收敛不确定结果，不建立通用operation repository，也不从execution snapshot反查身份。

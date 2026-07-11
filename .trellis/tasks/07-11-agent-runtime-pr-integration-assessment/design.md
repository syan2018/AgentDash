# Design · Agent Runtime 与 Workspace/Channel 集成

## 1. Decision Summary

- Agent execution 以 PR #93 efdfa5dc 的 Managed Runtime、AgentRun facade、Business Surface、
  Platform Tool Broker、Driver Host 与 RuntimeWire 为权威基线。
- 平台 capability 继续以 canonical Operation Gateway 为权威；Operation、OperationScript、
  shared Interaction、Extension 与 Channel 不进入 Managed Runtime aggregate。
- 主工作目录直接承载 PR 基线上的集成分支；当前 Workspace/Channel 分支放入旁路 source worktree，
  默认从该 worktree 物理搬运最终实现，只有 Runtime 桥接点重新接线。
- 不保留 RuntimeSession、旧 Canvas、Session-bound action gateway、旧 WorkspaceModule runtime bridge
  或双读/双写。

## 2. Worktree And Git Topology

    F:\Projects\AgentDash
      branch: codex/agent-runtime-workspace-integration
      base: efdfa5dc (PR #93 exact head)
      role: primary planning + integration implementation checkout

    F:\Projects\AgentDash-workspace-duplex-source
      branch: codex/workspace-duplex-interaction-planning
      role: immutable physical-transplant source
      anchor: 7070f6b0

拓扑准备：

1. 在当前存档分支暂存包含 untracked files 的规划目录。
2. 将主工作目录切换到 exact PR head 上的新 integration branch。
3. 为存档分支创建旁路 source worktree。
4. 在主工作目录恢复规划目录并完成最终审阅。
5. 规划提交直接落在 integration branch；审阅通过后在此执行 task.py start。

回滚锚点：

- efdfa5dc：纯 Agent Runtime 基线。
- 7070f6b0：旁路 source worktree 中的完整 Workspace/Channel 业务实现。
- 每个集成主题提交：单独回滚，不形成巨型 conflict-resolution commit。

## 3. Target Ownership

| Boundary | Canonical owner |
| --- | --- |
| Agent Thread/Turn/Item/runtime interaction/runtime operation | Managed Agent Runtime |
| AgentRun product coordinate and command facade | AgentRun application |
| Driver offer/binding/generation/lease/applied evidence | Integration Driver Host |
| Cross-process driver protocol | RuntimeWire |
| Platform capability identity/schema/admission/placement/result | Operation Gateway |
| Ephemeral Rhai composition | OperationScript executor |
| Shared UI state/command/event/attachment | Interaction subsystem |
| Extension installation/artifact/component/backend capability | Extension subsystem |
| Communication membership/message/delivery/binding | Channel subsystem |
| Agent-facing grouping/presentation | WorkspaceModule projection |

同名概念保持显式分层：

- AgentRuntimeOperation 是 Runtime command acceptance/recovery。
- OperationRef 是 provider-qualified platform capability。
- AgentRuntimeInteraction 是 Thread 内待响应请求。
- InteractionInstance 是长期 shared product state。

## 4. Runtime Bridge Ports

Runtime 内部类型只允许出现在 adapter/composition crate，不进入 Operation、Interaction、Channel 或
WorkspaceModule domain/application API。

### 4.1 Surface Compilation Port

    pub struct AgentOperationSurfaceTarget {
        pub run_id: Uuid,
        pub agent_id: Uuid,
        pub frame_id: Uuid,
    }

    pub struct AgentOperationSurfaceSnapshot {
        pub authority_revision: String,
        pub descriptors: Vec<OperationDescriptor>,
    }

    #[async_trait]
    pub trait AgentOperationSurfacePort: Send + Sync {
        async fn surface(
            &self,
            target: AgentOperationSurfaceTarget,
        ) -> Result<AgentOperationSurfaceSnapshot, AgentOperationSurfaceError>;
    }

Business Surface adapter 将 descriptor 稳定映射为 ToolContribution：

- contribution key、runtime tool name 与 Host mapping 均由 exact OperationRef 确定；
- Driver 只接收 name/schema；
- Host 保存 binding-scoped runtime_name -> OperationRef + authority_revision + descriptor_digest；
- active Interaction attachment、Extension installation 和 capability visibility 只影响编译结果；
- applied surface ack 成功后工具才可调用。

### 4.2 Tool Broker Executor Port

    pub struct BoundOperationToolInvocation {
        pub binding_id: String,
        pub generation: u64,
        pub tool_set_revision: u64,
        pub runtime_item_id: String,
        pub operation_ref: OperationRef,
        pub authority_revision: String,
        pub input: Value,
    }

    #[async_trait]
    pub trait BoundOperationToolExecutor: Send + Sync {
        async fn invoke(
            &self,
            invocation: BoundOperationToolInvocation,
            cancel: CancellationToken,
        ) -> Result<Value, BoundOperationToolError>;
    }

Adapter 从 AgentRun Runtime binding、current AgentFrame 与 binding-scoped mapping 构造可信
OperationInvocationCommand：

- principal：AgentRunAgent { run_id, agent_id }；
- scope：server-resolved Project/Workspace/Interaction scope；
- origin：AgentTool；
- trace：Runtime Thread/Turn/Item refs；
- idempotency：Runtime Item ID；
- attachment：current active Interaction attachment；
- placement：Operation provider/project/workspace binding resolver。

Operation Gateway 不依赖 Runtime contract；Tool Broker 不解析 provider/attachment/Extension。

### 4.3 Channel Delivery Port

    #[async_trait]
    pub trait ChannelAgentDeliveryPort: Send + Sync {
        async fn deliver(
            &self,
            delivery: AdmittedChannelDelivery,
            target: AgentRunTarget,
        ) -> Result<AgentRunMessageReceipt, ChannelDeliveryError>;
    }

PR AgentRun mailbox/facade 实现该 port。Channel 不持有 RuntimeThread，Managed Runtime 不复制 Channel。

### 4.4 OperationScript Nested Invocation

最终方案：

- Runtime 只持有顶层 OperationScript ToolCall Item；
- 每个 nested Operation 重新进入 OperationExecutionCore，并产生 child Operation trace/audit；
- nested call 继承 parent Runtime Item 作为 trace/idempotency provenance；
- allowed-operation manifest、descriptor/current authority、permission、effect/replay policy 每次重新校验；
- 需要 durable multi-step recovery 的流程继续使用 Workflow。

每个 nested call 不创建 child Runtime Item，因为这会让 Managed Runtime 开始拥有脚本内部 provider
调用、扩展 Runtime command/event 协议并增加不必要的恢复语义。

## 5. Physical Transplant Manifest

### A. Exact Snapshot Transfer

以下内容以 7070f6b0 最终目录快照为准，可使用 Git path restore/cherry-pick 物理搬运：

- crates/agentdash-domain/src/interaction/**
- crates/agentdash-application/src/interaction/**
- crates/agentdash-infrastructure/src/persistence/postgres/interaction_repository.rs
- crates/agentdash-contracts/src/interaction/**
- Interaction/Canvas frontend services、stores、features/canvas-panel/**
- Extension Component ABI、isolated host、component tests 与 package toolchain additions
- Canvas exact revision Extension package builder、SourceBundle digest tests 与 promotion E2E
- canonical Operation domain types、OperationScript ports/Rhai engine 的独立新增文件
- Channel V2 的独立 domain/provider/persistence modules 与 tests
- WorkspaceModule 最终 projection-only 新目录结构

搬运后只做 import、crate manifest 与生成注册修复，不重写业务逻辑。

### B. Snapshot Transfer Plus Integration Fixes

- Operation Gateway crate：以当前最终 tree 为源，重命名/定位为
  agentdash-application-operation-gateway，再接 PR composition。
- Extension action/protocol/backend：搬运 exact Operation provider，删除 PR 旧
  extension_actions/session_actions。
- Workflow OperationScript node/caller：搬运实现，适配 PR Workflow executor ports。
- Channel service/Companion：搬运 V2 service 后，将 delivery adapter 改接新 port。
- generated contract registry：合并 Rust source registration，最终统一重新生成产物。

### C. Bridge-Specific Reimplementation

仅这些路径按 PR 新接口实现：

- AgentFrame/Business Surface -> AgentOperationSurfacePort；
- binding-scoped Operation tool mapping；
- Platform Tool Broker -> BoundOperationToolExecutor；
- AgentRun trusted authority/trace resolver；
- MCP provider 对 RuntimeThread/Binding/Host placement 的解析；
- ChannelAgentDeliveryPort 的 AgentRun mailbox/facade adapter；
- API AppState/bootstrap/integration composition；
- migrations 0066/0067 与最终 specs。

### D. Deletion Wins

- PR 删除的 RuntimeSession crate、connector、RelayPrompt 与旧 mailbox/surface 文件不搬回。
- 当前分支删除的旧 Canvas aggregate/routes、WorkspaceModule runtime bridge/tools 和
  Session-bound Extension gateway 不搬回。

## 6. Migration And Contracts

- PR 0061–0065 原样保留。
- Channel V2 migration 重编号 0066。
- Interaction/Canvas replacement migration 重编号 0067。
- 空数据库直接得到最终模型；不 backfill、不提供旧 reader。
- 最终 Rust source 合并后统一运行 contracts generation：
  - 保留 Agent Runtime/Wire contracts；
  - 保留 Interaction、Extension、WorkspaceModule contracts；
  - 删除 Canvas legacy contracts；
  - generated TS/JSON schema 不从两分支手工拼接。

## 7. Dependency Direction

    domain interaction/channel/operation
            ^
    application use cases + operation gateway
            ^
    application ports
            ^
    agent-runtime-operation adapter / API composition
            ^
    Managed Runtime + Tool Broker + Driver Host

- Operation Gateway 不依赖 Managed Runtime crates。
- Interaction/Channel/WorkspaceModule 不依赖 Runtime contract/wire/host。
- Adapter 可以同时依赖 Runtime 与 Operation application ports。
- RuntimeWire 不增加 generic Operation、Extension 或 Channel frame。

## 8. Validation

关键端到端证据：

1. Agent attachment -> new surface revision -> applied ack -> tool call -> Tool Broker ->
   exact Operation -> Interaction state commit。
2. Agent OperationScript -> parent Runtime Item -> multiple child Operation traces，逐次 re-admission。
3. standalone Canvas/UserWorkshop 在无 AgentRun/Runtime binding 时调用相同 Operation Gateway。
4. Extension Component event -> exact Operation/OperationScript，artifact revision 保持 pinned。
5. Channel ingress/service admission -> AgentRun mailbox/facade -> Runtime Turn。
6. RuntimeWire disconnect 只 terminalize Runtime facts，不改变 Interaction/Channel canonical state。
7. fresh PostgreSQL migration 0061–0067、contracts、frontend 和 workspace gates。

## 9. Conflict Root-Cause Audit

55 个显式冲突不是同一种架构问题。从 Git mechanics 看，其中 24 个是 modify/delete，12 个是
spec/journal content conflict，真正的产品代码 content conflict 为 19 个，并集中在 API composition、
AgentRun surface、frame construction 与公共出口。按路径 ownership 再建立以下正交分类：

| 类别 | 显式冲突基线 | 初步判断 | 本次迁移目标 |
| --- | ---: | --- | --- |
| Specs / workspace 协作记录 | 12 | 反映两条分支都更新了权威文档和 append-only 记录，不属于产品模块耦合 | 从最终 source 重写 specs，按时间合并记录 |
| API / application / registry 集中装配 | 24 | AgentRun、repository、DTO、SPI、workflow 和 test support 汇聚在少数 root；部分共享合理，但文件承担的业务判断过多 | composition root 只注册 port/adapter，feature-local registrar 持有具体装配 |
| AgentRun / RuntimeSession / WorkspaceModule 旧接缝 | 19 | 业务重构仍修改具体 mailbox、surface、RuntimeSession、runtime bridge/tool 文件，是可消除的 Runtime 实现泄漏 | 采用三个窄 port；业务核心不再引用 Runtime contract/host/wire 类型 |

另有 24 个 modify/delete 冲突横跨上述类别，主要表示双方从同一旧入口并行 cutover；它是时间与提交
拓扑问题，不能单独证明目标模型错误。40 个自动合并的重叠路径仍需语义审计，避免编译成功后保留双
事实源。

具体根因、代表路径和独立性指标见 `research/coupling-root-cause.md`。需要被架构修正的重点是 Runtime
identity 扩散、AgentFrame/WorkspaceModule 承担过多集成职责、Channel 构造 mailbox 内部对象，以及
AppState/registry 过于扁平；并行 cutover、migration 编号和 generated/spec 冲突则按协调热点处理。

实施期维护一份逐路径审计表，至少记录：path、双方意图、根因类别、canonical owner、搬运/删除/重接
结论、最终依赖边和验证证据。它既用于指导 conflict resolution，也用于判断本次桥接是否真的降低了
下一轮 Runtime 重构的影响面。

### 9.1 Independence Invariants

- `agentdash-application-operation-gateway`、Interaction、Channel、WorkspaceModule 的 core manifest
  不依赖 `agentdash-agent-runtime*`、Runtime Host 或 RuntimeWire crate。
- RuntimeThread、RuntimeTurn、RuntimeItem、binding/generation 等类型只出现在 Runtime adapter 与 API
  composition allowlist；业务 port 使用自身稳定坐标。
- AppState、integration registry、contract generator 和 crate root 只做显式注册/导出；provider
  admission、authority、attachment、delivery 规则位于各自 owning module。
- 每个业务模块可用 fake port 独立运行 focused tests；替换 Managed Runtime adapter 不要求修改其
  domain/application tests。
- RuntimeWire 断开、重连或替换 driver 只改变 Runtime facts，不改变 Interaction/Channel canonical state。

### 9.2 Before / After Evidence

基线证据使用 `7070f6b0...efdfa5dc` 的 95 个重叠路径、55 个显式冲突、crate dependency graph 与
Runtime 标识符引用。完成证据包括：

1. 逐路径审计表全部关闭；
2. `cargo metadata` 依赖方向检查与 Runtime type allowlist 扫描通过；
3. physical transplant 与 bridge commits 分离，可证明业务目录没有夹带 Runtime 适配修改；
4. Surface、Tool Broker、Channel delivery adapter contract tests 覆盖所有必要集成行为；
5. 最终 diff 中不可避免的共享 root 修改均为注册、导出、migration 或 generated source orchestration。

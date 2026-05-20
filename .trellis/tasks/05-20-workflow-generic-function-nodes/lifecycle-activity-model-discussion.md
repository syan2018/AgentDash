# Lifecycle / Activity / Executor 抽象讨论

## 背景

当前 Workflow / Lifecycle 系统已经从“单个 Agent Session 的工作契约”扩展到“多个节点组成的 DAG”。随着 `agent_node`、`phase_node`、`function_node` 三类节点进入同一张图，系统开始暴露一个更基础的问题：

Lifecycle 究竟是在描述 Session 的生命周期，还是在描述一个面向目标的工作过程？

这个问题会直接影响后续设计。如果 Lifecycle 被理解为 Session 生命周期，那么所有节点都倾向于围绕主 session 的启动、切换和终止来建模；如果 Lifecycle 被理解为工作过程实例，那么 session、HTTP 请求、Bash 命令、人工判断都只是完成某个工作单元的不同执行方式。

更适合当前系统演进方向的定义是：

> Lifecycle 是一次目标导向的工作过程实例。它描述一组 Activity 如何按 DAG 依赖执行、消费输入、产出 artifact、完成或失败。Session 是 Lifecycle 的宿主、执行载体或观察面之一，但不是 Lifecycle 的本体。

## 当前模型的张力

现有两层模型本身是有价值的：

- `WorkflowDefinition` 描述单 session 的行为契约，包括 prompt、能力、注入、hook、port。
- `LifecycleDefinition` 描述多个 step 组成的 DAG，包括 edge、port、运行状态。

张力主要来自 `LifecycleStepDefinition.node_type` 同时承担了“节点是什么”和“节点如何执行”两类含义。

当前三类节点的语义并不在同一层：

| Node Type | 当前语义 | 所属层次 |
| --- | --- | --- |
| `agent_node` | 创建独立 child session，由 Agent 执行一个工作单元 | Activity 的执行方式 |
| `phase_node` | 在已有 root session 内切换 workflow contract / capability | Session runtime transition |
| `function_node` | 由平台直接执行 API / Bash 等确定性动作 | Activity 的执行方式 |

`agent_node` 与 `function_node` 都像可完成、可失败、可产出 artifact 的工作单元。`phase_node` 更像对某个 session 施加上下文变化。把三者并列为 node type 后，Lifecycle DAG 同时混入了“工作单元”和“运行时变更”，导致几个边界变得模糊：

- 一个 node 是否一定代表可独立观测的工作单元？
- 主 session 是 Lifecycle 的执行者、控制者、还是其中一个 Activity 的执行载体？
- step 的完成到底来自 session terminal、agent tool、hook、function result，还是统一的 activity event？
- `workflow_key`、`function`、`capability_config` 等字段如何组合才合法？

这些模糊点在新增 Function Node 后会被放大，因为 Function Node 不需要 workflow contract，却仍要共享 DAG、port、artifact、execution log、失败语义和调度机制。

## 建议的新定义

建议把 Lifecycle 的核心概念改名或重塑为 Activity Graph：

```text
LifecycleDefinition
  activities[]
    key
    description
    executor
    input_ports
    output_ports
    completion_policy
    context_policy
  edges[]
```

核心思想是：

- Node 表达 Activity。
- Activity 是 Lifecycle DAG 中可调度、可观测、可完成、可失败的工作单元。
- Executor 表达 Activity 由谁、在哪里、以什么方式执行。
- Session 是 Executor 可使用的一类执行载体，不是 Lifecycle 的本体。

对应的 executor 可以用强类型 tagged enum 表达：

```rust
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActivityExecutorSpec {
    Agent(AgentActivityExecutorSpec),
    Function(FunctionActivityExecutorSpec),
    Human(HumanActivityExecutorSpec),
}

pub struct AgentActivityExecutorSpec {
    pub workflow_key: String,
    pub session_policy: AgentSessionPolicy,
}

#[serde(rename_all = "snake_case")]
pub enum AgentSessionPolicy {
    SpawnChild,
    ContinueRoot,
    AttachExisting,
}

#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FunctionActivityExecutorSpec {
    ApiRequest(ApiRequestNodeSpec),
    BashExec(BashExecNodeSpec),
}
```

这样，当前三种节点可以重新映射为：

| 当前概念 | 新模型 |
| --- | --- |
| `agent_node` | `ActivityExecutorSpec::Agent { session_policy: SpawnChild }` |
| `phase_node` | `ActivityExecutorSpec::Agent { session_policy: ContinueRoot }` |
| `function_node` | `ActivityExecutorSpec::Function { kind: api_request / bash_exec }` |

`phase_node` 不再是独立 node type，而是 Agent Activity 的一种 session policy。它仍然表示“在主 session 内执行下一阶段”，但这个阶段本身仍是 Activity，因此可以拥有 input ports、output ports、completion policy、execution log 和明确的完成事件。

## 主 Session 与 LifecycleRun 的关系

建议把 `LifecycleRun.session_id` 明确定义为 root session：

```text
root session
  └─ owns / observes LifecycleRun
        ├─ Activity A: Agent / SpawnChild
        ├─ Activity B: Function / BashExec
        ├─ Activity C: Agent / ContinueRoot
        └─ Activity D: Function / ApiRequest
```

root session 的职责可以分为四类：

- **发起入口**：用户在主 session 中启动某个 LifecycleRun。
- **交互界面**：需要人工输入、确认或继续 Agent 对话时，主 session 可以承载这些交互。
- **上下文锚点**：project、workspace、权限、能力快照、审计 trace 可以挂在 root session 或其 owner 上。
- **执行载体之一**：当 Activity 使用 `AgentSessionPolicy::ContinueRoot` 时，root session 参与执行该 Activity。

这一定义让 child session 与 function execution 也拥有平等位置。它们不是主 session 生命周期的附属事件，而是 LifecycleRun 中某个 Activity 的执行实例。

## 推进模型

当前 orchestrator 更接近“session terminal 后触发 DAG 后继评估”。在 Activity 模型下，推进入口应该统一为 activity event：

```text
ActivityEvent
  ├─ AgentSessionCompleted
  ├─ AgentSessionFailed
  ├─ FunctionCompleted
  ├─ FunctionFailed
  ├─ HumanApproved
  └─ HumanRejected

ActivityEvent -> LifecycleEngine -> update run state -> schedule ready activities
```

Session terminal callback、agent tool、hook、function executor 都只是 ActivityEvent 的来源。真正的状态推进由 LifecycleEngine 统一处理：

- 校验 Activity 是否处于可完成 / 可失败状态。
- 校验 output port 是否满足 completion policy。
- 写入 output artifacts 和 execution log。
- 计算后继 Activity 是否 Ready。
- 将 Ready Activity 交给对应 executor scheduler。

这样可以避免每种执行方式各自调用 `complete_step` / `fail_step` 并各自触发后继调度，也能让连续 function activities、主 session continue activities、child session activities 走同一套可观测路径。

## 字段模型建议

预研阶段可以直接把 `LifecycleStepDefinition` 收敛为更强类型的 Activity 定义。示意：

```rust
pub struct LifecycleActivityDefinition {
    pub key: String,
    pub description: String,
    pub executor: ActivityExecutorSpec,
    pub input_ports: Vec<InputPortDefinition>,
    pub output_ports: Vec<OutputPortDefinition>,
    pub completion_policy: ActivityCompletionPolicy,
    pub capability_config: CapabilityConfig,
}
```

`workflow_key` 从顶层 optional 字段移动到 `AgentActivityExecutorSpec` 内。`function` 从顶层 optional 字段移动到 `FunctionActivityExecutorSpec` 内。这样非法组合会在类型层面减少：

- Function Activity 不携带 workflow key。
- Agent Activity 必须携带 workflow key。
- ContinueRoot 与 SpawnChild 都复用同一套 Agent workflow contract 解析。
- Function Activity 的配置由 function executor schema 负责。

`capability_config` 是否仍保留在 Activity 顶层，可以按语义拆分：

- Agent Activity：作用于对应 session 的 runtime capability。
- Function Activity：作用于 function executor 的平台能力约束，例如 workspace exec、network access、secret reference。

如果两者差异继续扩大，后续可以把它下沉到 executor spec 内。

## Function Node 任务的影响

当前“工作流通用功能节点扩展”任务仍然成立，但建议调整落地点：

1. 新增能力时优先引入 `ActivityExecutorSpec::Function`，而不是继续扩大 `LifecycleNodeType`。
2. API Request / Bash Exec 作为 Function executor kind，而不是顶层 node type。
3. Orchestrator 接入点从 `match node_type` 调整为 `schedule_activity(executor)`。
4. Function 执行完成后提交 `ActivityEvent::FunctionCompleted / FunctionFailed`，由 LifecycleEngine 统一推进。
5. Bash 执行所需的本机运行环境通过 executor runtime port 注入，不让 application 层直接依赖 API 层 provider 细节。

如果实现成本需要分阶段，可以先保留现有数据库字段名 `steps`，但在 domain 类型内把 step 语义重命名为 Activity。外部 API / TS 类型同步到新模型后，再决定是否重命名持久化字段。

## 迁移路径

可按三步推进：

### Step 1：概念收敛

- 在领域层引入 `ActivityExecutorSpec`。
- 将 `agent_node` 映射为 `Agent + SpawnChild`。
- 将 `phase_node` 映射为 `Agent + ContinueRoot`。
- 将拟新增的 `function_node` 映射为 `Function`。

这一阶段可以通过 serde schema 直接演进现有 JSON 结构；项目处于预研期，不需要保留旧 schema 的兼容路径。

### Step 2：执行引擎收敛

- 把 session terminal callback、agent advance tool、hook advance、function result 都归一成 ActivityEvent。
- 让 LifecycleEngine 负责完成 / 失败 / artifact / execution log / successor ready 计算。
- 让 executor scheduler 只负责启动对应 Activity，不直接拥有 DAG 推进规则。

### Step 3：UI 心智收敛

- DAG 节点展示 Activity。
- Inspector 的一级选择变成 Executor：Agent / Function / Human。
- Agent executor 内选择 session policy：spawn child / continue root。
- Function executor 内选择 kind：API request / Bash exec。
- 运行视图按 Activity 展示状态，再展开显示执行实例：child session、root session turn、function run、human decision。

## 需要继续讨论的问题

- `ContinueRoot` Activity 的完成事件应该由 agent tool 主动提交，还是由 root session 某个 terminal / phase boundary 自动提交？
- root session 同时作为交互入口和 Activity 执行载体时，是否允许多个 ContinueRoot Activity 并行 Ready？
- Function Activity 长时间运行时，LifecycleEngine 是否需要持久化 executor run id，以支持恢复和重试？
- Human Activity 是否应首版进入同一套 ActivityEvent 模型，还是先只为未来预留 executor kind？
- completion policy 应该仍由 output port existence / hook gate 表达，还是提升为 Activity 顶层策略？

## 倾向结论

Lifecycle 应该被定义为工作过程实例，而不是主 session 的生命周期。DAG 中的节点应该统一表达 Activity，Activity 通过 ExecutorSpec 选择执行方式。`phase_node` 更适合成为 Agent Activity 的 `ContinueRoot` session policy，而不是与 AgentNode / FunctionNode 并列的第三种节点。

这套抽象能解释当前系统已经拥有的能力，也能容纳下一步的 Function executor、Human approval、条件分支和更清晰的运行恢复机制。它让主 session 回到“宿主和执行载体之一”的位置，避免后续所有平台动作都被迫伪装成 session 行为。

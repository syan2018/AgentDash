# Decision Ledger

## Resolved Product Decisions

| ID | Decision | Evidence / Reason |
| --- | --- | --- |
| D-W01 | `OperationScript` 是无独立持久化的一次性 Rhai 脚本请求，由 Agent、Canvas 和 Workflow 复用同一 executor。 | 核心目标是让 Agent 像 JS REPL 一样组合 Operation 并筛选/清理结果；当前没有 script asset、job 或跨调用 REPL state 的产品需求。 |
| D-W02 | Application 定义 `OperationScriptEngine` port，Infrastructure 先接现有 bounded Rhai runtime；Canvas 可保存 Rhai source，但完整脚本始终交给服务端执行。 | 便于未来增加新 sandbox，而不改变调用合同；浏览器 iframe 不成为脚本 authority。 |
| D-W03 | 每个脚本内 `invoke` 都进入 canonical Operation execution core 并重新 admission；请求显式携带允许清单和 limits。 | 外层允许运行脚本不等于允许所有内部 effect，运行期间 capability 仍可能变化。 |
| D-W04 | RuntimeSession 不参与 RuntimeGateway authority、scope、surface 或 placement，并最终从 Canvas/Extension Gateway contracts 删除。 | Canvas/Extension/Interaction 必须能独立于 AgentRun 和历史 Session 执行。 |
| D-W05 | InteractionInstance 继承 definition scope：Personal → User-owned，Project → Project-owned；AgentRun 只是可选 attachment，explicit close + retention 管理生命周期。 | 共享状态不能依附临时 tab、renderer、AgentRun 或 RuntimeSession。 |
| D-W06 | Human 与 Agent 使用同一 typed command；Agent policy 只有 `direct` 与 `human_only`。 | `human_only` 拒绝 Agent canonical write；非权威建议经 Channel attention 传递，不在 Interaction 内建设 proposal aggregate。 |
| D-W07 | canonical state transition 由平台 Interaction service 确定性执行，只提供有限通用 state commands 与少量 typed handlers。 | 不建设 generic reducer registry、declarative DSL，也不允许 Extension/Canvas reducer code；外部副作用在 state commit 后通过 Operation/OperationScript 完成。 |
| D-W08 | Extension 贡献 Component + canonical Operation；Component 接收 props/state projection 并发出 typed event，由 definition binding 映射为平台 command 或 OperationScript。 | 既有 runtime actions、protocols 和 backend services 可复用为 Operation；Extension 自有复杂状态继续留在其 service/Operation。 |
| D-W09 | InteractionDefinition 使用 immutable revision + optimistic CAS；编辑草稿留在客户端。 | Human/Agent 都提交 base revision；不建设 durable draft、CRDT 或实时协同编辑。 |
| D-W10 | InteractionInstance 固定 definition revision 与 exact Extension artifact digest；升级只作用于新 definition/new instance。 | 既有 instance 不自动 rebind；artifact 缺失时 structured unavailable，不建设通用 state migration engine。 |
| D-W11 | 旧 Canvas 模型直接由最终 Interaction 模型整体替换。 | 项目没有需要迁移的 Canvas 存量；新 migration 删除旧聚合/runtime state tables、routes、DTO 和 repositories，不做 backfill 或兼容 decoder。 |
| D-W12 | Extension component 使用 descriptor + isolated iframe + scoped MessageChannel。 | 第三方 UI 需要明确的 props/events schema、CSP、资源限制和 capability membrane。 |
| D-W13 | 父任务使用 `work-items/` 管理落实步骤。 | 用户确认以单个父任务统一追踪依赖、状态和证据。 |
| D-W14 | V1 从第一天固定 definition/interaction/script host/component/Operation version discriminator；future breaking change 新增 V2 + 显式 migration。 | 用户明确本次之后需要承担兼容与迁移；pin revision 只有在 interpreter/handler 语义也被版本化时才真实有效。 |
| D-W15 | Canvas source 使用 immutable SourceBundle + base-revision VFS changeset；publish/copy/unpublish/promotion 固定 exact revision lineage。 | 当前 Canvas 已有多文件 VFS、Personal/Project distribution 和 promotion，最终替换必须完整承接这些产品语义。 |
| D-W16 | V1 canonical generic mutation 为 bounded `state_patch_v1`；Component event 只做 schema validation + payload pass-through。 | 固定 add/remove/replace、path allowlist 与 state schema，既能覆盖 host-owned JSON state，又不会演化成任意 reducer/transform DSL。 |
| D-W17 | OperationScript executor async、Rhai `rhai_v1` 语法同步；execution-scoped `ops.invoke` 隐式等待、`ops.invoke_all` 有界 structured concurrency。 | Rhai 1.24 evaluator/host functions 为同步 API；有界 worker + async bridge 满足 I/O 需求并保留未来 sandbox adapter 空间。 |
| D-W18 | Interaction command 的可靠副作用仅通过 replay-safe 单 `OperationEffectIntent` outbox；OperationScript 不自动 replay，复杂 durable multi-step effect 归 Workflow。 | 多步脚本可能产生不可安全回放的 partial side effects；项目现有 terminal-effect outbox 已证明 state fact 与可靠副作用应分层。 |
| D-W19 | Definition 声明 resource slots；Project/instance 与 attachment-local runtime binding 分层，binding handle 不是 capability。 | 当前 Canvas data binding 存在于 AgentRun VFS overlay；共享 runtime 必须区分共享可授权资源与 actor-local preview 资源。 |

## Technical Decisions Derived During Implementation

- `RuntimeInvocationEnvelope`、Operation descriptor 与 resolver 的具体 Rust 类型和 package ownership。
- Rhai host function JSON bridge、默认 limits、source/effect digest canonicalization 和 result store 复用点。
- Interaction tables/indexes、retention columns、`state_patch_v1` JSON Patch library 边界与 subscription transport。
- OperationEffectIntent store/claim/replay adapter 和 replay-policy descriptor 的具体类型。
- SourceBundle child rows/object storage、digest canonicalization、VFS batch changeset 与 lineage repository 的具体实现。
- exact artifact retention/addressability 的 repository 边界。
- RuntimeSession 与旧 Canvas 路径的精确删除 write set。

这些事项从已确认合同、现有代码和规范推导；如果实现证据要求改变产品语义，先回到本台账与根规划文档重新评审。

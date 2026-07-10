# Canvas / Interaction V1 冻结审查

## 结论

V1 应采用当前主方向：Canvas 保留为用户可见的 authoring/presentation 产品形态，底层由 `InteractionDefinitionRevision(kind=canvas)` 承载；共享运行态由 `InteractionInstance` 承载；工具组合由一次性 `OperationScript` 承载；调用与通信分别归 `Operation` 和 `Channel`。

这套边界适合作为项目首次稳定合同，原因是它把当前 Canvas 混合承担的代码资产、运行实例、Agent attachment、共享状态和浏览器 renderer 拆成生命周期不同的对象，同时没有引入持久化脚本资产、通用 reducer、通用迁移引擎或第二套 Workflow。

## 必须满足的事实

- Canvas 是 Personal/Project scoped 的多文件可运行资产，具有 VFS 编辑、data binding、publish/copy/unpublish、Extension promotion 和 Workspace Module presentation 行为。
- Canvas renderer 与 AgentRun runtime 都是临时执行面，不能拥有共享状态或资产身份。
- Agent 需要像受控 JS REPL 一样即时编写脚本组合多个工具，并对结果执行筛选、聚合和清理。
- MCP、ExtensionProtocol、Runtime Action 与 host action 必须经过同一个 Operation admission；脚本外层获准不能替代 nested invocation admission。
- 项目当前没有需要保留的 Canvas 生产数据，可以一次建立最终 V1 schema；V1 发布后则必须依赖显式版本合同和迁移策略演进。

## 主要替代方案

| 方案 | 结论 | 原因 |
| --- | --- | --- |
| 保留 Canvas aggregate，再旁挂 InteractionInstance | 不采用 | 资产身份、source revision、runtime state 和 Workspace Module projection 会形成两套事实源，未来迁移成本更高。 |
| 把 Canvas 完全做成 Extension package | 不采用 | Personal/Project authoring、VFS 编辑与共享实例会被迫依赖安装/打包生命周期。Extension 适合贡献 Component/Operation，不适合作为核心交互资产 owner。 |
| 把 Interaction command/event 并入 Channel | 不采用 | state revision/CAS 与 participant/message/delivery 的事务和恢复不变量不同。 |
| 把脚本持久化为 Program/Job aggregate | 不采用 | 当前目标是即时组合与结果处理；durable retry/gate/recovery 已由 Workflow 承担。 |
| 在 Rhai V1 中实现 Promise/原生 async syntax | 不采用 | Rhai 1.24 evaluator 与 host function 是同步模型；强行构造 resumable VM 会扩大实现和审计面。Executor 异步、脚本隐式等待即可满足需求。 |

## V1 冻结合同

### 版本与身份

- `InteractionDefinitionRevision` 固定 `definition_format_version`、`interaction_contract_version`、immutable source bundle digest 和 command/component/script bindings。
- OperationScript 请求固定 AgentDash dialect `rhai_v1` 与 `host_api_version=1`；保存于 Canvas/Workflow 的 source 依赖该版本，而不是直接依赖 Rhai crate 小版本。
- 平台 state handler 使用封闭、版本化 identity，例如 `state_patch_v1`、`instance_close_v1`；增加行为时新增版本，不改变既有实例语义。
- Workspace Module 分离 authoring 与 runtime identity：`canvas:{definition_id}` / `canvas://{definition_id}` 表达 Canvas definition，`interaction:{instance_id}` / `interaction://{instance_id}` 表达共享实例。

### Source、发布与资源绑定

- Canvas source 以 immutable `SourceBundle` 随 definition revision 固定，包含 entry、sandbox/import map、files、source digest；VFS changeset 使用 base revision CAS 生成新 revision。
- Personal → Project publish 从 exact source revision 创建或更新独立 Project-owned definition，并记录 lineage；copy 创建独立 User-owned definition；unpublish/archive 只移除目录可见性，仍被 instance/artifact/lineage 引用的 revision 保持可寻址。
- Extension promotion 从 exact definition revision/source bundle 生成 artifact。
- definition 声明 resource slots；instance/attachment runtime binding 将 slot 绑定到 authorized resource ref。共享 canonical effect 只能使用 instance/Project 可授权 binding，attachment-local binding 只服务该 actor 的 preview/runtime projection。

### Interaction command 与副作用

- V1 的通用 canonical mutation 是版本化 `state_patch_v1`：仅允许有界 `add/remove/replace` JSON Patch，受 definition 声明的 JSON Pointer allowlist、input/state schema、expected revision、patch count 和 state size 限制；整个 patch 与 event/state revision 在单个事务中提交。
- Component event binding 只做 schema validation 和 payload pass-through，目标只能是一个版本化 platform command，或一个即时 Operation/OperationScript action；不执行服务端任意 transform/reducer code。
- 即时 OperationScript 不自动 replay，允许返回 partial-call evidence；复杂 durable orchestration 归 Workflow。
- canonical command 若需要可靠外部副作用，只能在同一事务追加 replay-safe 的单个 `OperationEffectIntent` outbox。复杂多步副作用通过一个 `workflow.start` 类 Operation 进入 Workflow；outbox 不持久化/重放任意 OperationScript。

### OperationScript 异步执行

- Application `OperationScriptExecutor::run` 与 host-call port 是 async；Rhai V1 语法保持同步，`ops.invoke()` 隐式等待，`ops.invoke_all()` 提供有界 structured concurrency。
- evaluator 在有界专用 worker pool 运行；execution-scoped `ops` capability object 通过 request/response bridge 调用 async OperationExecutionCore，Tokio core worker 不阻塞。
- execution-scoped progress/cancellation/deadline hook 同时覆盖纯 Rhai CPU loop 和等待中的 nested Operation；达到 `max_concurrent_scripts` 时进入 admission/queue limit。
- preflight token 绑定 dialect/host API、source、input、allowed Operation descriptor/effect manifest、normalized limits、principal/scope 和 expiry；run 与每个 nested invoke 都重新 admission。
- V1 禁止递归调用 OperationScript；结果 ref 继承 caller owner/scope、capability 和 TTL。

## 冻结门槛

- 新 schema 和 contracts 从第一天携带显式 V1 discriminator；未来 breaking change 使用 V2 reader/handler 与显式 migration，不依赖字段猜测。
- Personal/Project publish/copy/unpublish、VFS changeset、resource binding、Extension promotion、definition preview 和 Interaction runtime 均有端到端验收场景。
- Interaction command transaction、OperationEffectIntent replay、Rhai cancellation/worker exhaustion、preflight token mismatch、artifact/source revision pinning 和 owner authorization 均有失败路径测试。
- repository-wide scan 确认旧 Canvas state authority、Session-bound runtime authority、synthetic Channel identity 和重复 Operation provider 已删除。

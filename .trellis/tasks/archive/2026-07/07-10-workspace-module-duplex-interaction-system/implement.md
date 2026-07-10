# Implement · Workspace Module 通用双工交互系统

父任务已完成实现与任务范围验收。WI-00 至 WI-10 均在同一个父任务目录内闭环；最终代码以主题化
commit 交付，规范、migration、generated contracts 与前端 consumer 同步收敛。

## 1. 工作项追踪

`work-items/README.md` 是实施状态、依赖和验收证据索引；每个 `WI-*.md` 维护自己的写入范围、退出条件、验证命令与进展记录。`implement.md` 表达全局顺序与集成合同，tracker 状态不能替代最终代码、规范和迁移检查。

| ID | 工作项 | 依赖 |
| --- | --- | --- |
| WI-00 | Architecture / contract gate | 已完成的产品决策与现状证据 |
| WI-01 | Canonical Operation 与 RuntimeGateway execution core | WI-00 |
| WI-02 | UserWorkshop runtime access 与 Canvas standalone bridge | WI-01 |
| WI-03 | OperationScript engine / executor / callers | WI-01；Agent caller 依赖 AgentRun adapter |
| WI-04 | InteractionDefinition / Instance / Attachment | WI-00 |
| WI-05 | Canvas 最终替换为通用 Interaction runtime | WI-02、WI-04 |
| WI-06 | Extension component ABI 与 runtime binding | WI-02、WI-04、Channel WI-01 ExtensionProtocol rename |
| WI-07 | Workspace Module 收束为 projection | WI-01、WI-03、WI-04、WI-05、WI-06 |
| WI-08 | Workflow / Channel 集成与端到端场景 | WI-03、WI-04、WI-06、Channel ref/admission 稳定 |
| WI-09 | RuntimeSession 依赖清除 | WI-02、WI-05、WI-06、AgentRun adapter 迁移 |
| WI-10 | 全量集成、spec、migration 与残留验证 | WI-01 至 WI-09 |

依赖只由本表和 `work-items/README.md` 表达；工作项编号不表示可以绕过依赖。

## 2. 全局实施清单

### WI-00 · Architecture / contract gate

- 固定 canonical Operation、`RuntimeInvocationEnvelope`、principal/scope/origin/placement/trace 和 actor-specific surface/admission 合同。
- 固定 OperationScript、Interaction owner/lifetime、Agent write policy、state transition ownership、definition concurrency、Extension artifact pinning 与旧 Canvas 替换合同。
- 固定 V1 discriminator、Canvas/Interaction module/presentation identity、SourceBundle/lineage/resource slot、`state_patch_v1` 和 OperationEffectIntent 合同。
- 更新 RuntimeGateway、Session、Capability、VFS、Frontend 与 cross-layer specs，使目标规范先于生产实现进入一致状态。
- 建立旧 Session-bound / Canvas-specific 合同到目标合同的唯一删除或替换矩阵，不保留兼容 alias 和双路径。

### WI-01 · Canonical Operation 与 RuntimeGateway execution core

- 定义 provider-qualified `OperationRef/Descriptor` 与 actor-specific catalog projection。
- 定义内部 `RuntimeInvocationEnvelope`，正交表达 principal、scope、origin、operation、authority revision、placement input 和 trace context。
- 建立 `RuntimeSurfaceResolver` 与 `RuntimePlacementResolver`；placement 从 Project/workspace/provider binding 解析，不从 RuntimeSession 或客户端 backend id 推导。
- 在 RuntimeGateway 内收束 direct invocation 与 OperationScript nested invocation 共用的 schema、capability、effect、cancel、trace、output validation、result ref 和 audit core。
- Operation descriptor 固定 exact version、effect 与 replay policy；result ref 固定 owner/scope/capability/TTL。
- Agent loop 保留外层 message/tool hook 与用户审批；OperationScript 的每次 `invoke` 仍使用允许清单并重新 admission。
- 首批 provider adapter 覆盖 MCP tool、RuntimeAction/host operation 和 ExtensionProtocol method。

### WI-02 · UserWorkshop runtime access 与 Canvas standalone bridge

- 建立 authenticated User + Project/Interaction access 到 RuntimeGateway 的 application adapter。
- discovery 返回 Operation descriptor、readiness 和非权威 revision/handle；每次 invoke 重新 admission。
- Canvas、Extension panel 与 Interaction renderer 复用同一 host bridge，不提交 RuntimeSession、AgentRun、AgentFrame、backend id 或 workspace root。
- Canvas 可保存 `.rhai` 或 inline Rhai source，并通过 host bridge 把完整脚本请求交给服务端 OperationScript executor；iframe 不解释执行该脚本。
- authoring/runtime public identity 分别固定为 `canvas:{definition_id}` / `canvas://{definition_id}` 与 `interaction:{instance_id}` / `interaction://{instance_id}`。
- 区分 Project asset preview 与 attached Interaction runtime preview。
- standalone preview 与 instance/attachment 使用同一 resource slot/binding resolver；attachment-local binding 不能成为 shared authority。
- 删除前端对已移除 `/canvases/{id}/runtime-snapshot` 的依赖，不恢复 legacy Session route。
- 补齐 agent input submit 对 interaction/render refs 的真实消费，或从合同删除未兑现字段。

### WI-03 · OperationScript engine / executor / callers

- 定义一次性请求 `{ language: "rhai_v1", host_api_version: 1, source, input, allowed_operations, limits }`；不建立独立 asset、execution aggregate、后台 job 或跨调用 REPL state。
- 在 Application 定义 `OperationScriptEngine` port 与 `OperationScriptExecutor`，Infrastructure 以现有 bounded `RhaiScriptRuntime` 实现首个 adapter。
- 抽出 execution-scoped evaluator factory：复用 AST cache/limits/JSON bridge，每次 run 注入 `ops` capability、progress/cancellation/deadline 与 counters；禁止依赖全局 helper context。
- evaluator 在受 `max_concurrent_scripts` admission 的专用 worker pool 中运行，通过有界 request/response bridge 调用 async OperationExecutionCore；Tokio core worker 不同步阻塞。
- 暴露 `ops.invoke` 隐式等待与有界 `ops.invoke_all` structured concurrency；脚本只能调用请求允许清单中的 canonical Operation，并禁止递归 OperationScript。
- preflight token 绑定 dialect/host API、source/input/manifest/limits/principal/scope/expiry；run 生成 ephemeral execution id，返回 partial/outcome-unknown call evidence。
- 实现调用次数/并行度/timeout/输出上限、CPU progress cancellation、caller cancellation、root/child trace 和 scoped result ref。
- Agent、UserWorkshop/Canvas 与 Workflow 调用同一 executor；每个嵌套 Operation 重新进入 WI-01 execution core。
- 不复制 Workflow durable runtime、human gate、retry、recovery 或 compensation。

### WI-04 · InteractionDefinition / Instance / Attachment

- 直接建立 immutable V1 `InteractionDefinitionRevision`、SourceBundle/lineage、atomic VFS changeset CAS、`InteractionInstance`、Attachment 和 RuntimeBinding。
- Personal definition 产生 User-owned instance；Project definition 产生 Project-owned instance。AgentRun 只是可选 attachment，explicit close + retention 决定 instance lifetime。
- 建立 command/event/state revision persistence；Human 与 Agent 使用同一 typed command use case、schema、expected revision、command idempotency 和 audit。
- definition command policy 仅为 `direct` 或 `human_only`；后者拒绝 Agent canonical write，Agent 可经 Channel 发送非权威 attention/suggestion，由 Human 重新提交 canonical command。
- 实现 `state_patch_v1` 的 add/remove/replace、JSON Pointer allowlist、input/state schema、patch/state size 与 atomic event/state transaction；其它行为只使用封闭 versioned platform handler。
- Component event binding 只做 schema validation + payload pass-through，明确区分 canonical command 与即时 Operation/OperationScript action。
- command 可靠副作用只写 replay-safe 单 OperationEffectIntent；dispatcher 重入 WI-01 core，复杂 durable multi-step effect 通过 Workflow。
- 建立 state/event subscription；User/Agent projection 与 RuntimeGateway access 不依赖 RuntimeSession。
- instance 固定 definition revision 与 exact Extension artifact digest；缺失 artifact 返回 structured unavailable。

### WI-05 · Canvas 最终替换为通用 Interaction runtime

- Canvas authoring/source/layout 直接成为 `InteractionDefinition kind=canvas` + immutable SourceBundle，不保留旧 Canvas 聚合、repository、route 或 DTO 作为兼容入口。
- Personal publish/Project unpublish/copy-to-personal 使用 exact revision lineage；archive 保留仍被 instance/artifact/lineage 引用的 revision。
- data binding 迁移为 definition resource slot + instance/attachment authorized RuntimeBinding；`bindings/*` 继续是只读 VFS projection。
- Extension promotion 从 exact revision/source digest 构建 package artifact。
- 运行时统一使用 InteractionInstance、Attachment、RuntimeBinding 与 renderer lease；reload 从 canonical instance state 初始化。
- AgentFrame 只保存 attachment/effective surface projection，不保存 Interaction state 或 Canvas definition body。
- WorkspacePanel tab/layout 继续作为 per-user presentation state。
- 通过新的 migration 创建最终 Interaction schema，并删除 `canvases`、`canvas_files`、`canvas_bindings`、`agent_run_canvas_runtime_observations`、`agent_run_canvas_interaction_snapshots`。
- 不回填旧数据、不提供旧 decoder/双读；fixtures、seed、examples 和 tests 直接按最终模型重建。

### WI-06 · Extension component ABI 与 runtime binding

- manifest/App definition 增加 `ui_components[]` descriptor、props/events schema 与 artifact validation。
- 实现 isolated iframe host、CSP、MessageChannel、schema validation、sizing、rate/size limits 与 instance-scoped bridge。
- component 接收 props/state projection，只发 typed event；Interaction definition 把 event 映射为平台 command 或 OperationScript 调用。
- Extension 贡献 Component + canonical Operation，不贡献 Interaction reducer；复杂业务状态继续由 Extension backend/service/Operation 持有。
- Canvas layout 保存 logical component contract ref；InteractionInstance 固定 exact installation artifact digest。
- artifact repository/cache 以 digest 地址并保留仍被 runtime binding 引用的 artifact；清理遵守引用与 retention。
- Extension upgrade 只影响新 definition/new instance；既有 instance 不自动 rebind，不建设通用 state migration engine。artifact 不可用时返回 structured unavailable。

### WI-07 · Workspace Module 收束为 projection

- Workspace Module 只组织 Agent-facing module/category/presentation 与 canonical Operation/Interaction refs。
- definition authoring 与 instance runtime 使用冻结后的不同 module/presentation identity。
- 删除 Workspace Module 内重复的 provider resolution、schema/admission、Canvas canonical state 和 Extension protocol dispatch 事实源。
- `list/describe/invoke/present` 对同一 resource 使用同一 actor-specific catalog、readiness 和 provenance。
- 静态扫描清除旧手写 operation/protocol/presentation DTO、weak parser 与 bypass resolver。

### WI-08 · Workflow / Channel 集成与端到端场景

- Workflow 增加 OperationScript node/call path，复用同一 executor，不复制脚本或 step runtime。
- Interaction attention 仅向 Channel 投递 typed reference/summary；Mailbox/外部 delivery 不成为 Interaction state authority。
- 场景一：Agent 与 Canvas 运行同一段 Rhai 脚本，覆盖允许清单、nested admission、cancel、timeout、trace/result 和结果筛选/清理。
- 场景二：host-owned interaction UI 覆盖 Human command、`state_patch_v1`、state/event、Agent direct/human_only、OperationEffectIntent replay 和 renderer reload。
- 场景三：Extension iframe component 覆盖 typed event binding、artifact/CSP/readiness，并触发 canonical Operation 或 OperationScript。
- 场景四：Personal publish、Project unpublish、copy-to-personal、resource binding、read-only shared VFS 与 Extension promotion 固定 exact source revision。

### WI-09 · RuntimeSession 依赖清除

- 将 Session-coupled Canvas/Extension/RuntimeGateway adapter 全部迁移到 principal/scope/origin/placement resolver。
- 删除 Gateway/provider request 中必填 session id、Session consumer variant 与以 Session 推导 backend/workspace authority 的路径。
- Runtime trace 使用通用 correlation refs；AgentRun/Interaction/Canvas/Extension 均可关联，runtime session id 不再是必填 root key。
- 删除前后端 legacy Session runtime routes、DTO、tests 与文档，不保留双路径或 fallback。

### WI-10 · 全量集成与规范收敛

- 运行所有 targeted、cross-layer、browser/integration、contract 与 migration checks。
- 更新相关 `.trellis/spec/` 的 Invariants、Current Baseline 与 contract appendices。
- repository-wide static scan 清除旧 Canvas aggregate/runtime snapshot、Session-bound Canvas/Extension Gateway contract、重复 Workspace Module provider 和旧 Extension Channel 词汇。
- 逐项核对 PRD acceptance criteria；不能只依据各工作项 tracker 的完成标记。

## 3. 验证策略

- OperationScript：`rhai_v1`/host API discriminator、AST/evaluator factory、plan-token mismatch、`ops` host surface、recursive-script rejection、worker exhaustion、CPU/host-call cancellation、nested admission、调用/并行/输出上限、partial outcome 和 scoped result ref。
- Interaction：SourceBundle/changeset CAS、`state_patch_v1` path/schema/size、expected-revision conflict、command idempotency、event ordering/state rebuild、OperationEffectIntent replay、direct/human_only、owner/lifetime、User/Agent projection 和 secret isolation。
- Runtime access：无 AgentRun/AgentFrame/RuntimeSession 的 Canvas/Extension invocation；拒绝客户端 authority injection；placement 从 Project/workspace/provider binding 解析。
- Component：CSP/origin/MessagePort/props/events schema/rate/size、artifact pinning、disable/upgrade readiness。
- Canvas distribution/binding：publish/copy/unpublish lineage、archive pinning、definition/instance identity、resource slot authorization、shared VFS read-only 与 Extension promotion。
- Migration：干净数据库直接形成最终 V1 Interaction schema；旧五张 Canvas/runtime state 表、旧 routes/contracts/repositories 和 compatibility decoder 均不存在。
- Workspace Module：canonical descriptor/provenance、describe/invoke parity、presentation、旧 resolver/parser/DTO 静态清扫。
- 全局：`cargo fmt --check`、受影响 package test/check/clippy、`pnpm run contracts:check`、`pnpm run frontend:check`、focused frontend/browser tests、`pnpm run migration:guard`、`git diff --check`。

## 4. 数据库 migration 合同

- 新增 migration 一次建立最终 V1 definition/revisions/source files or bundles/lineage、interaction instances/attachments/runtime bindings、commands/events/state revisions、OperationEffectIntent outbox 与必要索引。
- 删除旧 Canvas aggregate/runtime state 五张表及其约束；项目无需要保留的存量，不做 backfill、旧结构 decoder 或兼容读取。
- OperationScript 复用现有 trace/audit/result store，不新增通用 script asset/execution/job/step tables；OperationEffectIntent 是 Interaction command 事务的单 Operation outbox，不是 script execution persistence。
- AgentFrame 中重复的 visible Canvas/module/VFS projection 从 canonical attachment 重建。
- 已提交 migration 遵守 migration guard；本次结构变化通过新 migration 完成。

## 5. Review Gate

- 产品决策已全部记录在 `work-items/decisions.md`，实现未重新打开产品分支。
- RuntimeGateway、OperationScript、Interaction、Extension、Canvas 替换和 Channel 边界在代码与 specs
  中使用同一合同。
- migration、contracts、strict clippy、相关 Rust/TypeScript tests 与静态残留扫描通过；全仓门禁中
  与任务无关的 Agent loop、React effect lint 和 Story E2E 基线失败记录在 WI-10，未通过越界修改掩盖。

# Implement · Workspace Module 通用双工交互系统

本文件只记录评审通过后的实施切片；当前任务仍处于 planning，不启动生产代码修改。

## 1. 实施顺序

### W0 · 修正存量 Canvas 合同断口

- 作为本任务内置的小修直接完成，不另建兼容层或独立任务。
- 区分 Project asset preview 与 attached Interaction runtime preview。
- 删除前端对已移除 `/canvases/{id}/runtime-snapshot` 的依赖，不恢复 legacy Session route。
- 普通 Project Canvas preview 通过 standalone UserWorkshop surface 获取 authorized Operation catalog/invocation，不创建 AgentRun、AgentFrame 或 RuntimeSession。
- 补齐 agent input submit 对 interaction/render refs 的真实消费，或从合同中删除未兑现字段。

### W1 · Canonical Operation 与 RuntimeGateway execution core

- 定义 provider-qualified `OperationRef/Descriptor` 与 actor-specific catalog projection。
- 定义内部 `RuntimeInvocationEnvelope { principal, scope, origin, authority_revision, operation, trace }`，Canvas/Extension origin 与 User/Service principal 分离。
- 建立 `RuntimeSurfaceResolver` 与 `RuntimePlacementResolver`；placement 从 Project/workspace/provider binding 解析，不从 Session 或客户端 backend_id 推导。
- 在 RuntimeGateway 内收束 direct invocation/program step 共用的 schema/capability/cancel/trace/result core；Workspace Module 只做 projection，不成为重复 provider。
- 保留 Agent loop 外层 message/tool hook 与用户审批；Program preflight 生成固定 effect manifest，MVP 排除需要逐 step 交互批准的 operation。
- 接入 cancellation、timeout、output schema、result ref 与 parent trace。
- 首批 provider adapter：MCP tool 与现有 RuntimeAction/host operation；Workspace Module 暴露 canonical descriptor，ExtensionProtocol 在 Channel 改名任务完成后接入。

### W1b · Standalone UserWorkshop runtime surface

- 建立 authenticated User + Project/Interaction access 到 RuntimeGateway 的 application adapter。
- surface discovery 返回 Operation descriptors、readiness 与非权威 revision/handle；每次 invoke 重新 admission。
- Canvas、Extension panel 与 Interaction renderer 复用同一 host bridge，不提交 RuntimeSession、AgentRun、AgentFrame、backend_id 或 workspace root。
- Extension 用户交互以 User principal + Extension origin 执行；Extension 自治行为使用显式授权的 Installation service principal。
- AgentRun 另由 AgentFrame adapter 进入同一 Gateway，验证两条路径没有平行 catalog/admission。

### W2 · OperationProgram MVP

- 定义 v1 JSON DAG、normalization、definition/effect manifest digest 与无副作用 preflight。
- 实现同步有界 scheduler、caller root token、step state、root/child trace 与 result store。
- 暴露 Agent tools：preflight/run；取消复用外层调用 token。
- 增加 UserWorkshop 与 AgentRun adapter 的同一服务入口；不执行 iframe JavaScript。
- durable get/cancel 与后台 execution records 后置到明确的异步模式。

### W3 · InteractionInstance 与 attachment

- 直接建立 canonical InteractionDefinition revision，并将 Canvas source 迁入该 revision；不先建设临时 Canvas revision。
- 建立 instance、attachment、command/event/state revision persistence。
- 使用 expected revision、command idempotency 与 append audit。
- 建立 state/event subscription；Interaction attachment 与 Gateway access 不依赖 RuntimeSession。
- Workspace Module 投影 `interaction.get_state/command/wait_events`，并提供 explicit agent projection。

### W4 · Canvas 迁移到通用 Interaction runtime

- Canvas 保留 authoring/source/layout 职责，并实现 `InteractionDefinition kind=canvas`。
- AgentFrame 保存 attachment 对当前 Agent 的 effective surface projection；VFS/module/visible Canvas 从 canonical attachment 派生，Interaction state 与 Canvas definition 不进入 frame。
- renderer frame/generation 建模为 lease；reload 从 instance state 初始化。
- WorkspacePanel tab/layout 继续作为 per-user presentation state。

### W5 · Extension component ABI

- manifest/App definition 增加 `ui_components[]` descriptor 与 artifact validation。
- Workspace Module/component catalog 投影 exact package/artifact identity。
- 实现 isolated iframe component host、CSP、MessageChannel、schema validation、sizing 与 instance-scoped bridge。
- Canvas layout 支持 logical component contract ref、props/state 与 typed event binding；resolved artifact 只固定在 instance runtime binding。
- MVP component 只能发 typed event，由宿主唯一映射为 Interaction command 或 OperationProgram，不能直接请求 command/operation。

### W6 · 三个递进 vertical slices

- Program-only：AgentRun/UserWorkshop 分别运行同一个同步 DAG，覆盖 approval manifest、capability、cancel、timeout、trace/result，并证明 UserWorkshop 路径没有 RuntimeSession。
- Interaction-only：host-owned approval UI 覆盖 Human command → state/event → Agent observe/command → renderer patch 与重建。
- Component composition：替换为 Extension iframe component，覆盖 typed event host binding、artifact/CSP/readiness；最后再绑定既有 Program。

### W7 · Workflow 与 Channel 集成

- Workflow 增加 `OperationProgramNode`，复用 executor，不复制 step runtime。
- Interaction attention policy 仅向 Channel 投递 typed reference/summary。
- 验证 mailbox/external delivery 不成为 interaction state authority。

### W8 · RuntimeSession 依赖清除

- 将现有 Session-coupled Canvas/Extension/RuntimeGateway adapter 全部迁移到 principal/scope/origin/placement resolver。
- 删除 Gateway/provider request 中必填 session_id、Session consumer variant 与以 Session 推导 backend/workspace authority 的路径。
- Runtime trace 使用通用 correlation refs；AgentRun/Interaction/Canvas/Extension 均可关联，runtime_session_id 不再是必填 root key。
- 删除前后端 legacy Session runtime routes、DTO、tests 与文档；项目未上线，不保留双路径或 fallback。

## 2. 建议任务拆分

| 子任务 | 主要范围 | 前置 |
| --- | --- | --- |
| RuntimeGateway execution core + descriptor | runtime gateway、Agent outer policy、Workspace Module/MCP projection | 无 |
| UserWorkshop runtime access | user/project authority、surface/placement resolver、standalone API | Gateway envelope + descriptor |
| Canvas standalone adapter | Canvas preview/runtime bridge | UserWorkshop runtime access |
| Extension standalone adapter | Extension panel/component host bridge | UserWorkshop runtime access + ExtensionProtocol rename |
| OperationProgram IR/executor | application service、contracts、result/trace | Gateway execution core |
| Agent-facing program tools | Workspace Module/AgentRun adapter | Program executor + Gateway envelope |
| Interaction domain/persistence | domain/contracts/application/migrations | ownership 决策 |
| Canvas definition/attachment migration | Canvas/VFS/AgentFrame/API/frontend | Interaction domain |
| Extension component descriptor/toolchain | Extension package/contracts/generator | component ABI 决策 |
| Isolated component host | app-web/artifact route/security | descriptor + Interaction command API |
| Duplex demo/check | example Extension/Canvas/integration tests | 上述 vertical slice |
| RuntimeSession removal sweep | API/contracts/gateway/providers/frontend/specs | Canvas + Extension + AgentRun adapters 已迁移 |

Channel 术语任务可并行完成 ExtensionProtocol 改名与全局 Channel ref 收束；本任务不等待 IM provider adapter。

## 3. 验证策略

- Program IR normalization/digest、cycle/ref/schema/limit property tests。
- 每个 step 重新 admission，覆盖 capability revocation 与 TOCTOU。
- root cancellation/timeout 传播、并行上限、partial failure 与 large result ref。
- Agent 外层 program tool 审批固定 effect manifest，step 复用 RuntimeGateway capability/trace/result audit。
- Interaction expected-revision conflict、command idempotency、event ordering 与 state rebuild。
- User/Agent projection 权限、secret field 不泄漏。
- renderer reload/multi-tab/lease expiry 与 standalone access re-resolution。
- Canvas/Extension standalone invocation 在无 AgentRun/AgentFrame/RuntimeSession 条件下通过；客户端 authority injection 被拒绝。
- cloud/local placement 从 Project/workspace/provider binding 解析，Session correlation 缺失不影响执行。
- component CSP/origin/MessagePort/schema/rate/size/cancel/trace tests。
- Extension install disable/upgrade 对 pinned instance 的 structured readiness。
- 人机双工 browser/integration smoke。

## 4. 数据库 migration 关注点

- 新建 definition revisions、interaction instances、attachments、commands/events/state revisions。
- 同步 Program MVP 复用 trace/audit/result store，不新建后台 job/execution step tables。
- 现有 AgentRun canvas latest snapshot 表保留为 diagnostics projection或迁移后删除，不能继续充当 canonical state。
- 旧 Canvas source 在迁移时生成初始 definition revision；项目未上线，不引入无 revision 兼容读取。
- AgentFrame 中重复的 visible canvas/module/VFS projection 应由 canonical attachment 重建，避免多事实源。

## 5. Review Gate

- `OperationProgram` 独立合同与 inline 同步 MVP 已由用户确认。
- RuntimeSession 与 Canvas/Extension/RuntimeGateway authority、scope 和 placement 完全脱钩已由用户确认。
- 随后完成 Interaction owner/lifetime 与 Agent write policy 的逐项 brainstorm。
- PRD/design 收敛、相关 `.trellis/spec/` 加载并完成 curated implementation/check context 后，任务才能从 planning 进入 implementation。

# Handoff · Workspace Module 通用双工交互系统

## 1. 当前状态

- Trellis task：`workspace-module-duplex-interaction-system`
- 状态：`planning`，尚未执行 `task.py start`
- 本轮只更新任务规划产物，没有修改生产代码、数据库或 generated contracts。
- 现有代码审计已经覆盖 OperationProgram、Canvas/Extension UI、Channel 边界与跨任务一致性；后续会话可以直接从设计决策继续，仅在落实现有 adapter 时按需读取具体代码。

优先阅读顺序：

1. [prd.md](./prd.md)
2. [design.md](./design.md)
3. [research/current-state.md](./research/current-state.md)
4. [implement.md](./implement.md)
5. 相关 Channel 任务：[../07-10-channel-domain-boundary-refactor-evaluation/design.md](../07-10-channel-domain-boundary-refactor-evaluation/design.md)

## 2. 用户已确认的产品方向

### OperationProgram

- `OperationProgram` 是独立、actor-neutral、可版本化合同。
- 同步 MVP 接受 inline definition；Canvas、AgentRun 与未来 Workflow 调用同一 executor。
- MVP 是有界 JSON DAG，使用无副作用 preflight、effect manifest、调用方 cancellation token、root/child trace 与 result ref。
- 后台 execution、durable get/cancel、loop、任意脚本、retry/compensation 后置。

### RuntimeSession

- RuntimeSession 是应被移除的历史执行耦合，不进入目标架构。
- Canvas、Extension panel/component、Interaction renderer 必须能在没有 AgentRun、AgentFrame、RuntimeSession 时访问 RuntimeGateway。
- AgentRun 只是另一种 RuntimeGateway adapter；AgentFrame 只服务 AgentRun 的 effective surface，不是用户工作坊的前置对象。
- RuntimeSession 在迁移期最多作为可选 trace/delivery evidence，不能提供 authority、scope、Operation surface 或 execution placement，最终从 Gateway/provider/API contracts 中删除。

### 存量小修

- 后端已删除 `/canvases/{id}/runtime-snapshot`，但普通 Project Canvas preview 仍调用该 endpoint。
- 修复已纳入 `implement.md` W0：普通 preview 改走 standalone UserWorkshop surface；不恢复旧 Session route，不另建独立任务。
- 该问题不再记录在 `AGENTS.md`。

## 3. 目标 RuntimeGateway 边界

Canvas/Extension 是 invocation origin，不是 security principal。内部 envelope 正交表达：

```text
RuntimeInvocationEnvelope {
  principal,            // User / AgentRunAgent / WorkflowNode / ExtensionInstallation
  scope,                // Project / InteractionInstance / Workspace binding
  origin,               // Canvas / ExtensionPanel / ComponentEvent / AgentTool
  operation_ref + input,
  authority_revision,
  trace_context
}
```

主链：

```text
Authenticated User + Project/Interaction access
  -> UserWorkshop Adapter
  -> RuntimeSurfaceResolver
  -> RuntimeGateway admission
  -> RuntimePlacementResolver
  -> Operation provider

AgentRun + AgentFrame
  -> AgentRun Adapter
  -> same RuntimeGateway
```

关键不变量：

- browser 不提交 session_id、backend_id、workspace root 或预组装 capability。
- discovery surface handle/revision 只服务 UI 稳定性与 diagnostics；每次 invoke 重新 authorization/admission。
- local/cloud placement 来自 Operation provider 与 Project/workspace/provider binding，不来自 Session。
- Extension 用户点击使用 User principal + Extension origin；Extension 自治执行使用显式授权的 Installation service principal。
- component MVP 只能发 typed event，由宿主 binding 映射为 Interaction command 或 OperationProgram，组件不直接获得通用 invoke 权限。

## 4. 交互对象分层

```text
InteractionDefinitionRevision
  -> InteractionInstance(state/revision/command/event)
  -> InteractionAttachment(User / optional AgentRun)
  -> UserWorkshop Adapter or AgentRun Adapter
  -> RuntimeGateway

PresentationState + RendererLease
  -> 只负责 tab/layout/browser renderer
```

- Canvas 是 `InteractionDefinition` authoring format 与 presentation schema。
- `InteractionInstance` 是人机双工状态事实源。
- AgentFrame 是 AgentRun 的 effective runtime surface revision，不保存 interaction state 或 Canvas body。
- Workspace Module 是 Agent-facing projection，不是 Operation/Interaction 的第二事实源。
- Channel 只负责 attention/message/delivery，不拥有 Interaction command/event/state。

## 5. 后续实施切片与依赖

建议本任务保持 parent planning task；确认剩余产品决策后，再建立可独立验证的 child tasks。依赖顺序：

1. `RuntimeGateway envelope + Operation descriptor/execution core`
2. `UserWorkshop runtime access`，依赖 1
3. `Canvas standalone adapter + W0 preview repair`，依赖 2
4. `Extension standalone adapter`，依赖 2 与 Channel 任务的 ExtensionProtocol rename
5. `OperationProgram executor`，依赖 1；Agent-facing tools再依赖 AgentRun adapter
6. `InteractionDefinition/Instance/Attachment`，依赖 owner/lifetime 决策
7. `Canvas interaction migration`，依赖 3 与 6
8. `Extension component ABI/isolated host`，依赖 4 与 6
9. `RuntimeSession removal sweep`，依赖 3、4 与 AgentRun adapter 全部迁移

每个 child task 都要在自己的 PRD/implement 中写明上述依赖，不能只依赖 parent/child 树位置暗示。

## 6. 下一项需要与用户确认

最高优先级仍是 `InteractionInstance` owner/lifetime。

当前推荐：instance 继承 definition scope——Personal definition 产生 User-owned instance，Project definition 产生 Project-owned instance；AgentRun 永远只是可选 attachment。tab、AgentRun 或历史 RuntimeSession 结束都不删除 instance，由 explicit close/retention 管理。

选择 AgentRun-owned instance 会让清理更简单，但会重新引入本任务正在消除的跨会话与人机共享耦合；全部 Project-owned 又会扩大 personal interaction 的默认可见范围。

确认该项后，继续一次只问一个问题：

1. Agent 对 shared state 是直接 command，还是部分 command 需要 proposal/human confirm。
2. definition source 的 human/Agent 并发采用 revision/CAS 还是 draft model。
3. Extension 升级后 existing instance 的 artifact pinning 与显式 migration policy。

## 7. 续接方式

建议新会话使用 `trellis-continue` 读取当前 planning task，然后从第 6 节的 owner/lifetime 决策继续；在用户完成最终 review 前保持 planning，不执行 `task.py start`。

可直接交给新会话的提示：

```text
继续 Trellis planning task `workspace-module-duplex-interaction-system`。
先完整读取 `.trellis/tasks/07-10-workspace-module-duplex-interaction-system/handoff.md`、`prd.md`、`design.md` 和 `implement.md`。
已确认 OperationProgram 独立合同，以及 RuntimeSession 从目标 RuntimeGateway/Canvas/Extension 架构中移除。
以现有审计为基线，直接从 InteractionInstance owner/lifetime 这一产品决策继续；只有落地具体 adapter 时再读取对应代码，一次只问一个问题并同步更新 PRD。
```

## 8. 工作区交付状态

- 两个 `07-10-*` task 目录当前是未提交 planning artifacts。
- `AGENTS.md` 没有本任务改动。
- 已校验 task JSON、JSONL context 引用与 `git diff --check`；未运行代码测试，因为没有生产代码变更。
- 迁移到另一台机器前，需要通过 Git commit/push 或文件同步带走两个 task 目录；本轮未替用户创建 commit 或 push。

# 模块拓扑确认

## 结论

本轮审查证明 8 个候选 domain 适合作为对抗性 review 的主拓扑，但不应作为实现拆分的硬边界。它们是运行链路视角下的事实源分组；API routes、generated contracts、application facade、frontend feature/store 只作为具体链路证据，不单独成为第一性审查模块。

正式审查采用了两层视角：

- 4 份跨域审查：验证相邻 domain 的交界是否存在 owner 漂移。
- 8 份单域审查：避免跨域合并掩盖局部事实源和路径冗余。

最终综合以单域审查为主，跨域审查用于发现交界问题和去重。

## Domain Map

### 1. Orchestrated Work Surface

- 范围：Workflow / Lifecycle / Orchestration / Task / Companion / Routine gates。
- 核心事实源：`LifecycleRun`、`OrchestrationInstance`、`RuntimeNodeState`、`LifecycleGate`、Task plan facts、Routine dispatch ledger。
- 主要入口：workflow/lifecycle routes、task tools、companion tools/gates、routine executor。
- 审查产物：`research/05-orchestrated-work-surface.md`，跨域补充见 `research/01-orchestrated-runtime.md`。
- 判断：适合单独审查。Companion/Routine gate 与 AgentRun mailbox 有交界，需要在综合报告中单独归 owner。

### 2. Agent Runtime Session Surface

- 范围：AgentRun / RuntimeSession / RuntimeGateway / mailbox / conversation control / frame construction。
- 核心事实源：AgentRun workspace snapshot、AgentConversationSnapshot、mailbox envelopes/receipts、RuntimeSession delivery/trace、AgentFrame launch surface。
- 主要入口：AgentRun workspace routes、RuntimeSession routes、mailbox command routes、runtime launch planner。
- 审查产物：`research/06-agent-runtime-session-surface.md`，跨域补充见 `research/01-orchestrated-runtime.md`。
- 判断：适合单独审查。Launch command、delegate、mailbox steering 属于此 domain；dynamic action availability 与 Extension/WorkspaceModule 交叉。

### 3. Extension / Workspace Module Runtime Surface

- 范围：workspace-module / extension runtime / extension host / extension SDK/UI / canvas module runtime。
- 核心事实源：ProjectExtensionInstallation、extension runtime projection、WorkspaceModule descriptor、RuntimeGateway extension action invocation、local extension host workspace/process/env permission。
- 主要入口：workspace_module tools/routes、extension_runtime routes、canvas promote/runtime routes、local extension relay handler、extension SDK/UI。
- 审查产物：`research/07-extension-workspace-module-surface.md`，跨域补充见 `research/02-extension-authority.md`。
- 判断：Workspace Module 与 Extension Runtime 是同一产品模型的不同层，不应分开作为一级 review domain；Canvas promoted extension 是重要交界。

### 4. Authority & Capability Runtime

- 范围：PermissionGrant / policy / escalation / CapabilityResolver / tool catalog / MCP capability / VFS capability。
- 核心事实源：PermissionGrant、AgentFrame capability surface revision、CapabilityState baseline、AgentRun effective/admission view。
- 主要入口：permission grant service/routes、CapabilityResolver、session tool builder、AgentRun effective capability/admission ports。
- 审查产物：`research/08-authority-capability-runtime.md`，跨域补充见 `research/02-extension-authority.md`。
- 判断：适合单独审查。Contract 不是一级模块；只用于验证 typed projection 是否反向成为事实源。

### 5. VFS & Runtime Tool Surface

- 范围：VFS mount / providers / runtime tool composer / context file discovery / mount ownership。
- 核心事实源：VFS mounts、Mount capabilities、typed/metadata owner hints、runtime tool provider list、VFS discovery policy。
- 主要入口：VFS builders/providers/tools、SessionRuntimeToolComposer、context mount discovery、frame construction VFS projection。
- 审查产物：`research/09-vfs-runtime-tool-surface.md`，跨域补充见 `research/03-vfs-local.md`。
- 判断：适合单独审查。VFS 与 Local 的交界只应是 relay/materialization/root_ref payload，不能合并为一个模块。

### 6. Local Runtime & Relay Surface

- 范围：agentdash-local / relay protocol / command handlers / terminal / materialization / runner claim / desktop shell。
- 核心事实源：relay wire envelope、local domain handlers、LocalRuntimeConfig、machine identity、runner/desktop claim result、workspace root guard。
- 主要入口：local WebSocket client、LocalCommandRouter、domain handlers、Tauri shell commands、runner claim client。
- 审查产物：`research/10-local-runtime-relay-surface.md`，跨域补充见 `research/03-vfs-local.md`。
- 判断：适合单独审查。Tauri shell 残留是 Local/Placement 共同问题，owner 应下沉到 `agentdash-local`。

### 7. Project / Workspace / Backend Placement

- 范围：project / workspace / backend / local runner enrollment / machine and workspace identity / settings。
- 核心事实源：Backend identity、ProjectBackendAccess、WorkspaceDirectoryFact/Binding/Inventory、BackendExecutionLease、scoped settings。
- 主要入口：backend management、runner registration、workspace sync/routes、backend access routes、settings routes。
- 审查产物：`research/11-project-workspace-backend-placement.md`，跨域补充见 `research/04-placement-context.md`。
- 判断：适合单独审查。Workspace placement 与 Local runtime 有交界，但 ProjectBackendAccess 和 WorkspaceDirectoryFact 应作为 placement owner。

### 8. Knowledge & Context Surface

- 范围：skill assets / shared library / context construction / MCP presets / story and session context。
- 核心事实源：Project SkillAsset、Shared Library installed asset、SessionContextBundle、FrameLaunchEnvelope context bundle、MCP preset runtime binding。
- 主要入口：frame construction owner bootstrap/request assembler、skill asset service/VFS provider、context builder、MCP preset runtime resolver。
- 审查产物：`research/12-knowledge-context-surface.md`，跨域补充见 `research/04-placement-context.md`。
- 判断：适合单独审查。Shared Library 和 Project SkillAsset 主链路已健康；当前主要风险在 launch context finalization 和 VFS skill discovery identity。

## Non-Domain Layers

- API routes：只作为入口/DTO adapter 证据；若 route 拥有业务 transaction，则归到对应 domain 的 owner 问题。
- `agentdash-contracts` / generated TS：只作为跨层投影证据；不作为业务事实源。
- `agentdash-application` facade：只作为聚合/装配层证据；不作为独立业务模块。
- Frontend feature/store：只作为消费面和反向塑造风险证据；不单独泛化为模块。

## Dispatch Record

跨域报告：

- `01-orchestrated-runtime.md`
- `02-extension-authority.md`
- `03-vfs-local.md`
- `04-placement-context.md`

单域报告：

- `05-orchestrated-work-surface.md`
- `06-agent-runtime-session-surface.md`
- `07-extension-workspace-module-surface.md`
- `08-authority-capability-runtime.md`
- `09-vfs-runtime-tool-surface.md`
- `10-local-runtime-relay-surface.md`
- `11-project-workspace-backend-placement.md`
- `12-knowledge-context-surface.md`


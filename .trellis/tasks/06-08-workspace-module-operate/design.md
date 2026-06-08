# Design · Workspace Module 操作面 (invoke + present)

> Parent design: `.trellis/tasks/06-08-workspace-module-registry/design.md` §6-8、§10。研究依据：本任务 `research/01..06`。Child 1 已落地契约/聚合/只读工具。

## 1. 范围

在 Child 1 之上新增两个元工具：`workspace_module_invoke`（按 module 来源分支派发）与 `workspace_module_present`（best-effort GUI 推送）。不做管理 UI（Child 3）。

## 2. 关键决断（来自研究的 5 个张力点）

### D2-1 operation DTO 补结构化派发分量（消除字符串 parse hack）

研究/02：`WorkspaceModuleOperation` 目前只有扁平 `operation_key + origin`，channel method 名含驼峰（`readProfile`），不能整体当 `action_key`（后者拒绝大写），靠 `rsplitn` 反解析脆弱。

决策：给 `WorkspaceModuleOperation` **增补可选派发字段**（additive contract change，重生成 TS）：

```text
WorkspaceModuleOperation {
  ... 原字段 ...
  dispatch: WorkspaceModuleOperationDispatch   // 新增，承载来源专属路由分量
}
WorkspaceModuleOperationDispatch = 
  | { kind: "runtime_action", action_key }
  | { kind: "protocol_channel", channel_key, method_name }
  | { kind: "canvas", canvas_action }
  | { kind: "builtin", builtin_key }
```

由 Child 1 聚合层（`build_workspace_modules`）在构造时一并填好。invoke 工具据 `dispatch` 直接路由，**不再 split operation_key**。这符合 parent 风险条"禁止各 adapter 各自 JSON 拼接/字段绕过"。

### D2-2 invoke 分支派发

`workspace_module_invoke(module_id, operation_key, input)` execute() 流程：

1. `project_id_from_context` 取 project；重建 `build_workspace_modules`（现取现算，复用 Child 1）。
2. 按 `module_id` 找 module，按 `operation_key` 找 operation；找不到 → 结构化错误（unknown module/operation）。
3. **可见性**：复用 `WorkspaceModuleDimension.allows(module_id)`（capability 通道），不可见 → 拒绝。
4. **input schema 校验**：operation.input_schema 存在则校验 input；不匹配 → 结构化错误。
5. 按 `operation.dispatch.kind` 派发：
   - `runtime_action` → 构造 `RuntimeInvocationRequest{ action_key, context: Session{session_id, project_id}, target: Backend{backend_id}, input }`，走 `RuntimeGateway::invoke`（复用 RuntimeActionToolAdapter 构造样板）。
   - `protocol_channel` → 走 `ExtensionRuntimeChannelInvoker`（channel_key + method_name + input），不经 action_key。
   - `canvas` → 走 `RuntimeGateway::invoke`，actor=UserCanvas，action_key=canvas_action（研究/03：canvas runtime-invoke 本就是三行 gateway 调用，直接复用同一路径，不另起 service）。
   - `builtin` → 本轮 `unimplemented` 结构化错误（预留）。
6. trace：request 携带 trace；记录 module source + operation provenance + backend。

### D2-3 backend / session 来源（研究/04）

`ExecutionContext` 可取 project/session/backend/workspace。backend 解析优先级：
1. `context.session.backend_execution`（remote 显式）；
2. 否则 `vfs.default_mount().backend_id`（local）；
3. 都无 → 结构化错误"缺 backend target"（与现有 `ExtensionRuntimeActionProvider` 报错语义一致）。

抽成 helper，参考 HTTP 侧 `select_extension_invocation_workspace`。

### D2-4 provider 依赖注入

研究/04：`RelayRuntimeToolProvider` 当前不持 `RuntimeGateway` / channel invoker。需在其构造处注入 `Arc<RuntimeGateway>`（及 channel invoker，若独立），供 invoke 工具调用。list/describe（Child 1）不受影响。

### D2-5 present 事件复用（研究/05，零契约改动）

`workspace_module_present(module_id, view_key, payload?)`：
- 校验 module 可见 + ui_entry 含 view_key；无 → 可操作诊断（结构化错误，不静默）。
- 复用 `PlatformEvent::SessionMetaUpdate{ key: "workspace_module_presented", value: {module_id, view_key, uri, payload} }` + `inject_notification`，模板 `PresentCanvasTool`。
- 前端在 `workspace_module_presented` meta 上 `useWorkspaceTabStore.openOrActivate(...)`（extension webview → workspace tab；canvas → canvas tab）。前端接收最小改动落本 child；无前端目标时后端已返回诊断。

## 3. operation 归属 + schema 校验落点（研究/06）

- runtime_action 权限已在 `ExtensionRuntimeActionProvider` 内裁决，**不重复**。
- 本 child 新增的是：operation 是否属于该 module（步 2）、input schema 校验（步 4）、可见性（步 3）。全部在元工具 `execute()` 服务端侧完成，拒绝未知 operation。

## 4. canvas 操作投影对齐

Child 1 把 canvas 投影为 `binding.{alias}` 类 operation。本 child 校准：canvas module 的 invokable operation 用 `dispatch.kind="canvas"` + `canvas_action`，operation_key 即 canvas runtime action。若某 canvas 暂无可执行 action，则其 operations 为空、仅保留 ui_entry（present 可用、invoke 返回"无可调用 operation"）。调整在 `build_workspace_modules` 的 canvas 分支，保持单一 canonical。

## 5. 不做 / 边界

- 不做 Agent 主动 create/update canvas authoring（D2 留后；canvas 仅 read/present/invoke 现有能力）。
- 不做管理 UI、DB 列（Child 3）。
- builtin invoke 预留未实装。

## 6. 验收对应

| 验收 | 落点 |
|---|---|
| invoke 路由到 extension action 返回结果 | §2 D2-2 runtime_action |
| canvas 分支走现有 gateway 路径，无第二套逻辑 | §2 canvas + §4 |
| 未知 op/schema/权限/缺 backend 各自明确报错 | §2 步2/4 + §3 + D2-3 |
| Agent 不传内部 ID | §2 execute 内解析 project/session/backend |
| present 推送 + 无目标诊断 | §2 D2-5 |
| trace 可还原 source/provenance | §2 步6 |

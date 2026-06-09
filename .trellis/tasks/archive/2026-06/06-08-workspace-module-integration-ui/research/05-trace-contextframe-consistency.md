# Research: trace / ContextFrame 一致性检查点（Child 1/2 已实现）

- **Query**: Child 1/2 provenance/trace 在哪记录；ContextFrame/session feed 是否需额外表达 workspace module
- **Scope**: internal
- **Date**: 2026-06-08

## Findings

### Files Found

| File Path | Description |
|---|---|
| `crates/agentdash-application/src/workspace_module/tools.rs` | Child 2 的 list/describe/invoke/present 工具 + trace/provenance 记录 |
| `crates/agentdash-spi/src/connector/mod.rs:206-337` | CapabilityDimension::WorkspaceModule + WorkspaceModuleDimension |

### trace / provenance 记录点（workspace_module/tools.rs）

- `RuntimeTrace` 来自 `runtime_gateway`（tools.rs:32 import），invoke 走 RuntimeGateway 后拿回 `result.trace`。
- `invocation_result_to_tool_result(result, provenance)`（tools.rs:334-）把：
  - `provenance: serde_json::Value`（module source / operation provenance）落进 details（R5 审计），key `"provenance"`（行 347、361）。
  - `runtime_trace`：`serde_json::to_value(&result.trace)`（行 339、575）落进 details key `"runtime_trace"`（行 349、363、589）。
- 三条派发分支（RuntimeAction / ProtocolChannel / Builtin）各自构造 provenance（行 495 起），并在 details 注入 `operation_origin` / `backend` 等（行 537-546、576-588、625-634）。
- protocol channel 分支自建 `RuntimeTrace::new()`（行 556），因为不经 RuntimeGateway。
- TraceInfo 还经 `.with_trace(TraceInfo {...})`（行 851）附到工具结果。
- 测试断言 provenance 可还原 `operation_origin` / `backend`（行 1288-1295）。

**即 trace/provenance 在 Agent 工具调用侧（invoke）已统一记录**。HTTP invoke 路由（如有）与 MCP 工具应共用同一 `invocation_result_to_tool_result`，避免两套 trace 形态——这是 R3"统一 trace 字段"的检查点。

### ContextFrame / session feed 表达

- **未发现** workspace module 在 ContextFrame / session feed 中有专门表达字段。Child 1/2 只在：
  1. capability 维度（`WorkspaceModuleDimension`，SPI 行 292）—— 可见性裁切。
  2. 工具调用结果 details（provenance + runtime_trace）—— 审计。
- session feed / ContextFrame 现状对 workspace module **无独立 segment / event 类型**；extension/canvas 的 UI 表达走各自的 workspace tab / canvas panel 渲染，不在 context feed。
- R3"ContextFrame 中 workspace module 的表达"应是**轻量确认**：当前无遗漏的必需点——裁切经 capability、调用经 trace details，二者已是 canonical。若要在 session feed 增加"module 调用"可见性，是新增表达（设计取舍），非修补遗漏。

### 诊断点（R3 UI 诊断）

- module unavailable：`build_workspace_modules` 已在 bundle 缺失时把 `status = Unavailable(reason)`（`workspace_module/mod.rs:258-262`），UI 直接读 `summary.status` 即可呈现诊断。
- present 失败 / invoke 失败：经工具结果的 error / trace details 表达；UI 侧诊断需消费这些字段（前端目前无对应展示）。

## Caveats / Not Found

- 未做全仓 ContextFrame 类型遍历；结论基于 Child 1/2 实现文件 + capability/SPI grep。若 design 要在 session feed 增"module 调用事件"，需另查 session event contract（prd.md 候选修改面已列 `session/platform event contract`）。
- HTTP 侧是否已有 workspace-module invoke 路由未确认（Child 2 可能只暴露了 Agent 工具/MCP 侧）；如要统一 trace，需对照 routes/extension_runtime.rs 的 invoke handler 与 tools.rs 的派发是否复用同一 application 逻辑。

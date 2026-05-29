# 结构性拆分设计

## 目标

本 child 处理仍真实存在的结构性问题：

- 建立 `agentdash-application-ports` crate，先迁移不依赖 application use case 的 port 类型。
- 收敛 application 内部对 backend/runtime/VFS transport 的依赖方向，避免新 crate 反向依赖 application。
- 将 session test support 从 production module 面剥离。
- 复核前端 god-component 与 feature 循环，只处理上一 child 未覆盖的残留。

## 已完成交叉项

`frontend-server-state-refactor` 已完成：

- `SettingsPageContent.tsx` 255 行。
- `activity-inspector.tsx` 336 行。

这两项不再作为本 child 的代码目标，仅在验收里记录交叉完成证据。

## 后端 Ports Crate

新增 crate：`crates/agentdash-application-ports`。

首批只迁移纯 port / DTO：

- `backend_transport.rs`：`BackendTransport`、`RelayPromptTransport`、relay session event/route DTO、workspace probe DTO、`TransportError`。
- `runtime_gateway/extension_actions.rs` 中的 extension action/channel transport trait 与 transport error。
- `vfs/materialization.rs` 中的 `VfsMaterializationTransport` trait。

不迁移 provider、service、use case、gateway registry，原因是这些模块消费 repository、runtime gateway 或 VFS service，属于 application 编排层。

DDD 边界：domain 不依赖 contracts / protocol DTO；ports crate 表达 application 边界 port，允许依赖 domain、relay、agent protocol 等内层或协议事实，但不允许反向依赖 application。协议层或 API 层负责把 domain/application 输出映射成 wire DTO。

## Session 目录

本批不做大规模 session module move。`session-assembly-converge` 刚重排过 composition 入口；继续大搬家会与刚落地的 helper 冲突。

本批只处理当前最明确的错误放置：`memory_persistence.rs` 是 test support，应该受 `#[cfg(test)]` 约束，不作为 production module surface。最终采用 `crates/agentdash-application/test-support/session_memory_persistence.rs`，并通过 `session/mod.rs` 的 test-only path module 暴露。

## 前端

当前仍需复核：

- `SessionChatView.tsx` 1008 行。
- `features/workspace/workspace-list.tsx` 的实际行数与拆分状态。
- `extension-runtime` / `workspace-panel` / `canvas-panel` 之间的交叉 import。

前端实现策略：

- 已低于 600 行的 Settings / Activity Inspector 不再修改。
- 若 `SessionChatView` 或 `workspace-list` 仍超过 600 行，则按 UI 子块拆成同目录 presentational 组件，不改变 store/service 行为。
- feature 循环通过中性 `features/workspace-runtime` 承载 workspace runtime data/context 与 tab descriptor contract 解除；不引入新全局 store。

## 验证

- `cargo check --workspace`。
- `cargo test -p agentdash-application --lib`。
- `pnpm -C packages/app-web exec tsc --noEmit`。
- grep 验证 `agentdash-application-ports` 为 workspace member，application 不再定义已迁 port。
- 行数验证前端目标组件，记录已完成交叉项和剩余项。

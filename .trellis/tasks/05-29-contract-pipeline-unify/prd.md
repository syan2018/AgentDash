# 契约双流水线统一（api/dto→contracts ts-rs→前端 codegen 单源）

> parent: `05-29-slop-cleanup-wave2`。盲审病灶 1。类：丙（前轮仅做 McpTransportConfig 单点）。Wave 2。

## Goal

消灭项目并存的两套契约系统，让 Rust↔TS 契约**单一 codegen 源 + CI 校验**，杜绝 domain 改字段→Rust 编译通过→前端运行时错的静默漂移类。

## 现状证据

- 两条流水线：
  - **生成线（真用）**：`contracts` crate `#[derive(TS)]` → `generate_contracts_ts` → 前端 `generated/*.ts`（确认被 import）。
  - **手同步线（漂移源）**：`api/dto/*.rs` **9/11 是 Serialize-only 手写 struct + 手写 `From<Domain>`**（如 `dto/task.rs::TaskResponse`），不进 contracts、不进 ts-rs；前端 `types/index.ts` 再手抄一份 TS（`Task/TaskStatus/StoryStatus/Workspace/Project/...`）。
- `Task` 全栈被重新定型 3 次（domain entity → api `TaskResponse` 手 `From` → 前端手写 type），跨语言零机器校验。
- 前端 `services/` 堆 identity mapper（`extensionRuntime.ts` ~30、`session.ts`、`workflow.ts`），把 generated 类型逐字段重建零转换；而 `projectVfsMounts.ts` 直接 `api.get<T>()` 信任 wire——证明 mapper 是选择非必需。
- `contracts` 重抄 domain 枚举（`mcp_preset.rs`/`vfs.rs`）且已漂移（`MountCapability` 4 vs 6，swarm S3 先修漂移本体留此删）；`CapabilityDirective` 前端三定义（swarm S7 先收）；`JsonValue` generated 9 份。

## 已拍板决策（2026-05-29）

- **契约单源 = `contracts`**：ts-rs/schemars 从 domain 逐出（见 `domain-purification`）；`api/dto` 的 9/11 response 提升为 `contracts` 的 `#[derive(TS)]` 类型，contracts 成为 Rust↔TS 唯一 codegen 边界。
- **前端信任 wire**：删 services identity-mapper，内部端点直接 `api.get<GeneratedType>()`，**不引入 zod/valibot 运行时校验**。

## Scope

1. 把 `api/dto` 的 9/11 Serialize-only response 提升为 `contracts` 的 `#[derive(TS)]` 类型并注册进 `generate_ts.rs`；handler 改用 contracts 类型（与 `api-handler-thinning` 协调）。
2. `generate_contracts_ts --check` 接入项目默认 gate：仓库当前没有 `.github` CI 配置，因此先要求根 `package.json` 的 `check` 链路包含 `contracts:check`，未来 CI 复用该脚本。
3. 删前端 `types/index.ts` 中与 generated 重复的手抄部分（`Task/TaskStatus/StoryStatus/Workspace/Project/...`）；真正无契约的端点全部补进 contracts，目标是 `types/index.ts` 不再手写 domain 概念。
4. **删 services identity mapper 层**：内部端点改 `api.get<GeneratedType>()`，删 `extensionRuntime.ts`/`session.ts`/`workflow.ts` 等的逐字段重建 mapper（信任 wire 决策）。
5. generator 发射共享 `JsonValue`（`common.ts`），消 9 份重复。
6. 删 contracts 对 domain 枚举的纯镜像副本（`McpTransportConfig`/`MountCapability`/`ProjectVfsMountContent`），改 codegen 单源。

## 依赖与协调

- swarm S3（MountCapability 漂移）、S7（CapabilityDirective/JsonValue 前端三定义）先行收敛快速面，本 child 处理根因（删副本本体 + codegen 统一）。
- 是 `domain-purification`（ts-rs/schemars 去向）的前置。

## Acceptance Criteria（硬指标 + 验收命令）

- [ ] `Task`/`Story`/`Workspace`/`Project` 在 contracts 有 `#[derive(TS)]`（grep）；前端 `rg "interface Task|type TaskStatus|interface Workspace|interface Project" packages/app-web/src/types/index.ts` = **0**
- [ ] 根 `package.json` 默认 `check` 链路包含 `contracts:check`，且 `pnpm run contracts:check` 本地通过
- [ ] services identity-mapper 删除：`extensionRuntime.ts`/`session.ts`/`workflow.ts` 中逐字段重建函数数量降至「保留清单」内（清单逐项注明为何需转换；其余 = 0）
- [ ] `rg "export type JsonValue" packages/app-web/src/generated | wc -l` = **1**（共享单源）
- [ ] contracts 纯镜像副本删除：`rg "struct McpTransportConfig|enum MountCapability|struct ProjectVfsMountContent" crates/agentdash-contracts` = **0**
- [ ] `cargo check --workspace` + `tsc --noEmit` exit 0；前端运行 smoke：列表/详情页契约消费无报错

### services mapper 保留清单（执行时填写，空表示全删）

| mapper | 保留理由（确需转换的字段） |
|---|---|
| （执行中按需补） | |

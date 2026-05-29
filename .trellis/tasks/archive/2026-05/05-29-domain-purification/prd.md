# domain 净化（逐出 ts-rs/schemars / session id newtype 或删）

> parent: `05-29-slop-cleanup-wave2`。盲审病灶 7（G）。类：丙。Wave 4，**依赖 `contract-pipeline-unify`** 定调。

## Goal

让 domain crate 不再被前端序列化/TS 绑定塑形，去掉假装成类型安全的 `=String` alias。

## 现状证据

- `domain/Cargo.toml` 依赖 `serde`/`serde_json`/`schemars`/`ts-rs`；48 文件 derive serde；`ts-rs::TS` 在 10 个 `workflow/value_objects/*.rs`；`schemars::JsonSchema` 进核心实体（`workspace/entity.rs:17`）。→ domain 被前端 TS 导出需求塑形，方向反了；也使 contracts 的镜像副本显得无意义。
- `session_binding/session_id.rs:42-59`：`SessionId`/`StorySessionId`/`ChildSessionId` 全 `pub type = String`，编译器视作同型，可互传；doc 自称"语义仍传达"但零编译保障，且 `LifecycleRunRepository::list_by_session`(`workflow/repository.rs:123`) 收 `&str` 不用 alias——自相矛盾。

## 已拍板决策（2026-05-29）

- **ts-rs/schemars 逐出 domain**：TS 导出全部由 `contracts` 承接（与 `contract-pipeline-unify` 决策一致）。
- **session id alias 删除**：执行复核后采用允许降级方案，删除 `SessionId`/`StorySessionId`/`ChildSessionId` 假 alias，字段保留 `String` 并由字段名表达语义。原因是这些 id 当前没有额外不变量，真 newtype 会牵动 repository、Postgres bind/read、API mapper 与大量 fixture，却不能提供对应收益。

## Scope

1. 把 `ts-rs` + `schemars` derive 全移出 domain；导出给 TS 的 workflow value object 改在 `contracts` 镜像/derive。`serde` 若持久化确需则保留，否则评估收窄。
2. 删除 session id 假 alias；repository 继续用当前事实类型，避免引入没有不变量收益的样板类型。

## 依赖

- **前置**：`contract-pipeline-unify`（决定 ts-rs/schemars 在 contracts 侧如何承接，避免 domain 移出后 TS 导出断裂）。

## Acceptance Criteria（硬指标 + 验收命令）

- [x] `rg "ts-rs|schemars" crates/agentdash-domain/Cargo.toml` = **0**；`rg "derive\(.*TS|JsonSchema" crates/agentdash-domain/src | wc -l` = **0**
- [x] TS 导出经 contracts 单源仍完整（`pnpm run contracts:check` 通过，前端无缺失类型报错）
- [x] `rg "pub type (SessionId|StorySessionId|ChildSessionId) = String" crates/agentdash-domain` = **0**（已删 alias，journal 记录降级理由）
- [x] repository 签名按选定事实类型保留；`SessionId` / `StorySessionId` / `ChildSessionId` alias 引用已清零。
- [x] `cargo check --workspace` + `pnpm -C packages/app-web exec tsc --noEmit` exit 0

# domain 净化（逐出 ts-rs/schemars / session id newtype 或删）

> parent: `05-29-slop-cleanup-wave2`。盲审病灶 7（G）。类：丙。Wave 4，**依赖 `contract-pipeline-unify`** 定调。

## Goal

让 domain crate 不再被前端序列化/TS 绑定塑形，去掉假装成类型安全的 `=String` alias。

## 现状证据

- `domain/Cargo.toml` 依赖 `serde`/`serde_json`/`schemars`/`ts-rs`；48 文件 derive serde；`ts-rs::TS` 在 10 个 `workflow/value_objects/*.rs`；`schemars::JsonSchema` 进核心实体（`workspace/entity.rs:17`）。→ domain 被前端 TS 导出需求塑形，方向反了；也使 contracts 的镜像副本显得无意义。
- `session_binding/session_id.rs:42-59`：`SessionId`/`StorySessionId`/`ChildSessionId` 全 `pub type = String`，编译器视作同型，可互传；doc 自称"语义仍传达"但零编译保障，且 `LifecycleRunRepository::list_by_session`(`workflow/repository.rs:123`) 收 `&str` 不用 alias——自相矛盾。

## 已拍板决策（2026-05-29）

- **ts-rs/schemars 逐出 domain**：TS 导出全部由 `contracts` 承接（与 `contract-pipeline-unify` 决策一致）。
- **session id 升真 newtype**（默认选定）：`SessionId`/`StorySessionId`/`ChildSessionId` 改 newtype struct，接受 `.0` 样板换编译期安全；repository 签名统一用 newtype 而非裸 `&str`。（若执行中发现 newtype 改造面过大，可降级为"删 alias 直用 String + 字段名"，但须 journal 记录理由 + 标人工复核。）

## Scope

1. 把 `ts-rs` + `schemars` derive 全移出 domain；导出给 TS 的 workflow value object 改在 `contracts` 镜像/derive。`serde` 若持久化确需则保留，否则评估收窄。
2. session id 升真 newtype（见决策）；repository 签名统一。

## 依赖

- **前置**：`contract-pipeline-unify`（决定 ts-rs/schemars 在 contracts 侧如何承接，避免 domain 移出后 TS 导出断裂）。

## Acceptance Criteria（硬指标 + 验收命令）

- [ ] `rg "ts-rs|schemars" crates/agentdash-domain/Cargo.toml` = **0**；`rg "derive\(.*TS|JsonSchema" crates/agentdash-domain/src | wc -l` = **0**
- [ ] TS 导出经 contracts 单源仍完整（`generate_contracts_ts --check` 通过，前端无缺失类型报错）
- [ ] `rg "pub type (SessionId|StorySessionId|ChildSessionId) = String" crates/agentdash-domain` = **0**（升 newtype；若降级删 alias 则同样 = 0 且 journal 记录）
- [ ] repository 签名用选定类型（`rg "list_by_session\(.*&str" ` 等裸 &str 入口收敛，journal 记录）
- [ ] `cargo check --workspace` + `tsc --noEmit` exit 0

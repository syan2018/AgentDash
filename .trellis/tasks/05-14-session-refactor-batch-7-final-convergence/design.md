# Design：Batch 7 Final Convergence

## Boundary

本批补齐父任务原始 Batch 7 的代码收口，不再把纯文档验证批次视作最终完成。

`SessionHub` 在当前分支仍保留为外部协调入口，但以下职责已经从 Hub 主体拆出：

- owner / construction / launch command / launch execution。
- runtime registry / turn supervisor。
- terminal effect outbox dispatcher。
- pending runtime command store。

本批要在代码层继续收掉剩余隐式行为：

- `working_dir` 从“join 后交给 connector”改为显式策略校验。
- legacy pending meta column 从 repository 主线移除。
- persistence 大 trait 拆出可依赖 store 能力边界。
- AppState 构造过程用 ready builder/ready binding 收束延迟注入。
- `SessionHub` 保留 public shell 时，只允许转发到已拆出的能力服务。

## Verification Matrix

- `cargo fmt --check`
- `cargo check -p agentdash-application`
- `cargo check -p agentdash-api`
- `cargo check -p agentdash-infrastructure`
- `cargo check -p agentdash-local`
- `cargo test -p agentdash-application session::hub`
- `cargo test -p agentdash-application session::terminal_effects`
- `cargo test -p agentdash-application session::runtime_commands`
- `cargo test -p agentdash-application session::memory_persistence`
- `cargo test -p agentdash-infrastructure terminal_effect_outbox_persists_status_transitions`
- `cargo test -p agentdash-application session::path_policy`
- `rg -n "pending_capability_state_transitions_json" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-infrastructure/src`

## Remaining Non-Blocking Follow-up

若仍无法完全删除 `SessionHub` 类型，必须将原因限定为 public call-site 迁移成本，并在本批至少完成可执行的服务边界拆分与新增行为入口禁令。不能再把“文档记录 follow-up”当作代码收口完成。

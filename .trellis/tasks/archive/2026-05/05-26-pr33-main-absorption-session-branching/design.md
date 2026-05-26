# Technical Design

## Branch And PR Flow

当前 `codex/pr33-main-merge-check` 只作为本地合并演练分支使用。真实工作应回到 `codex/session-tree-branching`，并在该分支上合入 `origin/main`。这样后续修正可以直接进入 PR #33，而不是停留在本地检查分支。

合入前需要保护演练结果：当前 staged 状态包含 PR33 与 main 的 no-commit merge 结果，并已包含 `context_envelope` 解析吸附到 checkpoint 层的冲突解决。后续执行可以用临时本地 commit、patch 或直接从演练分支 checkout 指定文件的方式保留这份已验证的冲突解法。

## Projection Conflict Resolution

`ContextProjector` 的边界是读取 session events、projection head、compaction metadata，并组装 `AgentContextEnvelope`。Projection segment 的内容 materialization 属于 checkpoint / compaction 解析层。

因此合并 main 时采用以下职责分配：

- `compaction_checkpoint.rs` 解析 `summary_chunk` checkpoint 和 `context_envelope` messages。
- `context_envelope` 解析出的 entries 必须标记为 projection origin、synthetic，并保存 segment id、source range 和原始 provenance。
- `context_projector.rs` 调用 `projection_entries_from_checkpoint_records`，不再内联 context envelope message 解析。
- `fork_initial_projection` 可以作为 projection head 覆盖判断中的特殊 strategy，但不得扩散成第二套解析路径。

## Backend Semantic Invariants

Rollback 的目标边界应以当前模型可见 projection head 为准。物理事件 head 只说明事件已存在，不说明模型上下文仍可见。二次 rollback、branch restore、fork restore 必须围绕 projection head 表达清楚。

Active compaction 只有在 projection 已 committed 时才可成为恢复上下文的事实来源。Range 覆盖是必要条件，但不是充分条件。

Fork point 有三种定位方式：event seq、message ref、compaction id。实现应避免 event seq 与 compaction id 描述不同边界。预研阶段不做兼容回退，推荐对混合输入做强校验或直接拒绝不一致请求。

## Project Session Lineage

项目会话列表只需要直接 parent relation，不需要 ancestors 与 children。当前 `lineage_view` 适合详情页，不适合列表页批量读取。收口方案可以分两步：

- 先把错误从 `.ok()` 静默吞掉改为显式返回 API 错误。
- 如改动成本可控，再新增 lineage store 的 bulk/direct-parent 查询，避免 N 次详情视图读取。

## Frontend Relation Model

`parent_relation_kind` 是前后端共享事实。前端列表不能继续把所有 `parent_session_id` 子会话称为 companion。

分组模型应从 `companions` 演进为 relation-aware children，例如 `linkedChildren` 或 `relatedChildren`，每个 child 保留 `parent_relation_kind`。展示层根据 relation kind 决定 label、title、aria 文案和视觉轻重。

## Trellis Metadata

父任务 `04-08-session-tree-branching` 仍记录旧 base branch。PR33 切回 main 后，应同步父任务和本子任务 metadata，使后续接力时不会继续按 stacked PR 处理。

## Validation Strategy

本任务默认遵守用户当前约束：不主动跑编译 / 全量测试。验证先覆盖：

- `git status --short --branch`
- `git diff --check`
- `git diff --name-only --diff-filter=U`
- 静态扫描 `parent_relation_kind` / `companion` / `lineage_view` / rollback / fork 边界关键字

如用户后续允许，再补最小 targeted tests 或 type-check。

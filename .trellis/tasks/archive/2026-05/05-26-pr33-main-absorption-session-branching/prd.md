# PR33 main 吸附与会话分支语义收敛

## Goal

让 PR33 的会话分支与投影回退能力回到真实 PR 分支上继续推进，并吸附 `main` 已经合入的 context compaction 主线结构。任务完成后，PR33 应以 `main` 为 base，fork / rollback / lineage / 前端 relation 展示语义一致，且不保留 stacked PR 的旧基线假设。

## User Value

- 后续 review 能直接在 PR33 内看到真实变更，不再依赖本地合并演练分支。
- 会话分支能力使用同一套 projection / compaction / lineage 语义，避免 fork 或 rollback 恢复出错误上下文。
- 项目会话列表能区分 companion、fork、rollback branch 等不同父子关系，减少 UI 心智偏差。

## Confirmed Facts

- 远端 PR #33 head 是 `codex/session-tree-branching`，原 base 是 `codex/context-compaction-infrastructure`。
- `main` 已合入 context compaction infrastructure，PR33 应切换为以 `main` 为基线继续处理。
- 本地已在 `codex/pr33-main-merge-check` 做过 no-commit merge 演练，唯一冲突集中在 `context_projector.rs` / `compaction_checkpoint.rs` 的 projection segment 解析职责。
- 已确认的冲突吸附方向是：`compaction_checkpoint.rs` 解析 `summary_chunk` 与 `context_envelope`，`context_projector.rs` 负责读取 projection head 并组装 context。
- 合 PR 前质量门已切到真实 PR 分支执行；格式、contracts、backend、frontend 检查均在主仓库分支上收口。

## Requirements

- 在不丢失当前合并演练成果的前提下，切回真实 PR 分支 `codex/session-tree-branching`。
- 将 `origin/main` 合入 PR 分支，并复用已经确认过的冲突吸附方向。
- 将 PR #33 base 从旧 stacked branch 调整到 `main`。
- 修正 rollback 的 projection head 边界校验，避免二次 rollback 将模型可见 head 推回已经隐藏的事件之后。
- 校验 rollback 继续引用的 active compaction 必须是可用于 projection restore 的 committed compaction。
- 明确 fork point 与 `fork_point_compaction_id` 的组合语义，禁止或强校验不一致的混合输入。
- 将项目会话列表的父子关系模型从“所有 parent 都是 companion”吸附为 relation-aware 模型。
- 收敛项目列表读取 lineage 的方式，至少避免静默吞掉 lineage 查询错误。
- 同步 Trellis 任务元信息，使原 `04-08-session-tree-branching` 与本子任务反映真实 PR base 和后续状态。

## Acceptance Criteria

- [ ] 当前合并演练结果被安全保留，切分支前没有未解释的 staged 冲突状态。
- [ ] `codex/session-tree-branching` 已合入 `origin/main`，且 PR33 可改 base 到 `main`。
- [ ] `context_envelope` projection segment 的解析归属在 checkpoint 层，projector 不重复承担 segment materialization。
- [ ] rollback 不能越过当前模型可见 head，也不能把未 committed compaction 写成 active projection head。
- [ ] fork 请求中 event seq 与 compaction id 的关系被明确约束，无法生成越界的 child initial projection。
- [ ] `parent_relation_kind` 被前端列表分组和展示消费，fork / rollback branch 不再被统一叫作 companion。
- [ ] 项目会话列表的 lineage 查询失败不会静默降级为无 parent。
- [ ] `.trellis/tasks/04-08-session-tree-branching/task.json` 与本任务 metadata 反映 `main` 作为 PR target。
- [ ] 按用户约束完成 `git diff --check`、冲突状态检查、静态吸附扫描，并在真实 PR 分支上完成除用户指示排除项外的合并前质量门收口。

## Out Of Scope

- 重新设计 session compaction 架构。
- 引入兼容旧 DTO 或旧数据库字段的回退路径。
- 处理与 PR33 无关的 UI 重构、运行时重构或历史任务归档。
- 在用户未允许前执行 `pnpm dev`、cargo build/check 或全量测试。

## Open Questions

- Playwright 关键 e2e 当前卡在 webServer 启动参数契约，已按用户指示不纳入本轮修复范围。

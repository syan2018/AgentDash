# 执行计划

## 顺序

1. 新增 migrations 与 domain entities。
2. 新增 repository traits / implementations，并接入 repository set。
3. 补 DTO / generated contract export，只暴露 refs 与 read views，不暴露 builder internals。
4. 编写 backfill：
   - `LifecycleRun.lifecycle_id` -> root `WorkflowGraphInstance`。
   - `LifecycleRun.session_id` -> root `LifecycleAgent` + root `AgentFrame.runtime_session_refs`。
   - `LifecycleRunLink` -> whole-run `LifecycleSubjectAssociation`。
   - 可确定的 agent spawn relation -> `agent_lineages`。
5. 修复 `SessionMeta.project_id` 的 Postgres create/get/list/save 路径。
6. 增加 repository-level 验证：run -> graph instances、run -> agents -> frames、subject -> associations、runtime session -> frame/agent/run trace lookup。

## 质量门

- 新 schema 可以表达同一 `LifecycleRun` 下多个 `WorkflowGraphInstance`。
- root backfill 后，存在 `session_id` 的旧 run 至少能找到一个 root agent 与 root frame。
- `LifecycleRunLink` 的旧数据可以通过 `LifecycleSubjectAssociationRepository` 查询。
- `SessionMeta.project_id` 持久读写有测试或等价验证。
- 没有新增业务入口继续依赖 `LifecycleRun.session_id` 作为 owner truth。

## 验证命令

- `python ./.trellis/scripts/task.py validate 06-01-session-lifecycle-target-anchors-schema`
- 针对迁移和 repository 的 backend test。
- `rg -n "project_id" crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs`
- `git diff --check -- .trellis/tasks`

## 后续交接

- `lifecycle-dispatch-service` 使用这些 repositories 创建或复用 run/graph/agent/frame/association/gate。
- `agent-frame-construction-migration` 接管 frame revision 内容与 launch projection。
- `workflow-agent-assignment-migration` 接管 assignment 与 attempt/claim/terminal 的语义一致性。

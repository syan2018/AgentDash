# 数据库业务语义收敛设计

## Scope

本任务聚焦跨层语义改造，不处理纯 dump 美化或 P0 baseline 正确性修复。目标范围来自父任务报告：

- Session runtime head：`sessions.project_id`、`executor_config_json`、`tab_layout_json`、`last_*` projection。
- Lifecycle run ledger：`active_node_keys`、`execution_log`。
- Legacy UI/config：`views`、`user_preferences`。
- Backend local runtime identity：`backends` local machine/share/claim 字段。
- Business redundancy：`stories.task_count`、`project_agents.is_default_for_task`、`projects.visibility/is_template`。
- Typed config/audit cleanup：LLM credential naming、permission grant JSONB 查询、canvas binding source contract。

## Boundary

父任务 `database-semantic-baseline-audit` 继续负责：

- 修复干净库 baseline/code 不一致。
- 删除高置信历史 default/residue。
- 将 `0001_init.sql` 整理为 hand-curated baseline。

本任务负责：

- 需要 repository/domain/API/frontend 同步改造的业务语义收敛。
- 需要产品语义确认或跨层 contract 重生成的字段删除/迁移。
- durable spec 更新。

## Target Model

### Session

`sessions` 只表达 RuntimeSession identity、event sequence head、display title projection 和 connector trace pointer。业务归属经 `runtime_session_execution_anchors -> AgentFrame -> LifecycleRun -> LifecycleSubjectAssociation` 查询；provider/executor 行为属于 AgentFrame execution profile；UI layout 属于 scoped UI state。

### Lifecycle

`lifecycle_runs` 是 run ledger。Activity active state 来自 `lifecycle_workflow_instances.activity_state_json`；execution audit 使用 append-only event 表，再投影到 `LifecycleRunView`。

### UI / Settings

用户偏好统一走 `settings(scope_kind, scope_id, key)` 或专门 scoped UI state。Saved backend views 若仍存在，必须带 user/project scope，不属于 `BackendRepository`。

### Backend Runtime

Backend registration config、local identity、share scope、runtime claim lifecycle 应在命名和约束上分离。若继续共表，列名和 CHECK 必须表达其事实类型。

## Risks

- Session/lifecycle 改造会影响 generated contracts 和前端视图，需要分阶段验证。
- 删除 `views` / `user_preferences` 前必须确认旧 backend repository port 的所有消费者。
- `stories.task_count` 和 `project_agents.is_default_for_task` 虽像冗余，但 contracts 暴露过，删除需要前端同步。


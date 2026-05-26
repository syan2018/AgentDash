# 会话分支与状态投影设计

## Design Goal

会话分支不是消息数组上的 UI 功能，而是 Session runtime 的状态投影能力。设计目标是让每个 branch 都能回答三个问题：

- 它从哪个 parent 的哪个 projection point 分出来？
- 当前模型可见 head 在哪里？
- restore 时应该使用哪个 checkpoint 和 suffix？

## Core Model

推荐继续沿用父任务确立的三层结构：

```text
session_events
  -> session_compactions / session_projection_segments
  -> session_lineage / session_projection_heads
```

### session_events

不可变审计日志，记录 Backbone / Platform feed。fork、rollback、branch status change 都应写入结构化 platform event，便于 UI 和审计追踪。

### session_compactions

模型可恢复 checkpoint 表面。父任务已经把结构性 compact 固化为 `session_compactions` 与 `session_projection_segments`，branch 场景继续复用这套形态：

- `compaction_id`
- `session_id`
- `source_start_event_seq`
- `source_end_event_seq`
- `first_kept_event_seq`
- `base_compaction_id`
- `branch_id`
- `status`
- `replacement_projection_json`

fork child 的 initial compaction checkpoint 可以把 parent fork projection 固化到 child 自己名下。这样 child restore 不依赖 parent 后续 retention。

### session_lineage

表达会话树：

```text
child_session_id
parent_session_id
relation_kind
fork_point_event_seq
fork_point_ref
fork_point_compaction_id
status
created_at_ms
metadata_json
```

`relation_kind` 用来区分：

- `fork`：用户或系统从某个历史投影分支。
- `companion`：companion / spawned child session。
- `spawned_agent`：由 agent 控制面产生的协作 agent session。

### session_projection_heads

表达当前模型可见状态：

```text
session_id
projection_kind
head_event_seq
active_compaction_id
updated_by_event_seq
updated_at_ms
```

初始至少支持 `projection_kind = model_visible`。UI 如果需要展示不同于模型可见状态的浏览 head，可以后续添加 `ui_visible`。

## Fork Flow

```text
resolve parent active projection at fork point
  -> create child session meta
  -> insert session_lineage edge
  -> materialize child initial compaction checkpoint
  -> initialize child projection head
  -> emit branch_forked platform event
```

关键约束：

- fork point 必须稳定引用 parent 事件边界。
- child initial compaction checkpoint 写入成功前，不应返回 fork 成功。
- child session 可以继续使用自己的 `session_events` 追加 suffix。
- parent 后续事件不会改变 child initial checkpoint。

## Rollback Flow

```text
validate rollback target within current session projection
  -> append rollback platform event
  -> update session_projection_heads.model_visible
  -> move model-visible projection head to the rollback target
  -> emit branch_rolled_back platform event
```

rollback 不删除事件。查询 active checkpoint 时必须同时满足：

- compaction `session_id` 匹配；
- compaction `status` 有效；
- compaction source range 不越过 `projection_head.head_event_seq`；
- suffix replay 只读取 projection head 允许的事件边界。

## Restore Flow

```text
load projection head
  -> load active compaction and projection segments
  -> build replacement projection entries
  -> replay session_events suffix after first_kept/source_end until head
  -> ProjectedTranscript
```

fork child restore 直接从 child initial compaction checkpoint 开始。`session_lineage.fork_point_*` 负责解释来源和支持审计；模型恢复不需要重新依赖 parent 的 live projection。

## API Shape

建议新增 session branch use cases，而不是塞进普通 session meta：

- `create_session_fork(parent_session_id, fork_point)`
- `rollback_session_projection(session_id, rollback_target)`
- `list_session_lineage(session_id)`
- `list_session_children(session_id, relation_kind?)`
- `read_session_projection_head(session_id)`

Backbone / Platform event 可新增：

- `session_branch_forked`
- `session_projection_rolled_back`
- `session_lineage_status_changed`

## Frontend Contract

前端使用 lineage API 显示 parent / child / branch 状态。`parent_session_id` 仍可用于 companion 兼容显示，但通用 branch UI 必须来自 `session_lineage`。

初版 UI 建议：

- Session header 展示 fork source。
- Session list 支持 parent-child grouping。
- Branch panel 列出 siblings / children / ancestors。

## Migration Notes

项目仍在预研期，直接创建目标 schema。PostgreSQL 和 SQLite 需要同步：

- `session_lineage`
- fork / rollback 相关 platform events
- child initial compaction metadata 与 projection segment 类型约定

## Trade-offs

- fork 时 materialize child initial checkpoint 会增加写入成本，但换来 child 独立恢复和更简单的 retention。
- rollback 用 projection head 表达会让查询逻辑更严格，但保留完整审计历史，也能避免物理删除事件。
- 独立 lineage 表让模型比单个 `parent_session_id` 更重，但可以同时表达 fork、companion、spawned agent 等不同关系。

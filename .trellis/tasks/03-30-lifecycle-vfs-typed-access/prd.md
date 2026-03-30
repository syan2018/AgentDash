# Lifecycle VFS 按类型访问语法扩展

## 背景

当前 `lifecycle_vfs` mount 只支持按 UUID 路径读取 artifact（`active/artifacts/{uuid}`）。
workflow context binding 中的 `latest_checklist_evidence` locator 暗示了一种需求：**按 artifact_type 过滤 + 取最新**。这在 agent 需要引用上游产物时很常见。

来源：context-binding-simplification PRD 的 P4c 讨论项。

## 初步设想

在 `lifecycle_vfs` provider 中增加虚拟路径语法：

```
active/artifacts/by-type/{type_tag}          → 返回该类型最新 artifact 内容
active/artifacts/by-type/{type_tag}/list     → 返回该类型所有 artifact 列表（JSON）
```

其中 `type_tag` 对应 `WorkflowRecordArtifactType` 的 snake_case 值：
`session_summary` / `journal_update` / `archive_suggestion` / `phase_note` / `checklist_evidence` / `execution_trace` / `decision_record` / `context_snapshot`

## 待决策

- 语法细节（`by-type` vs `@type` vs query parameter 风格）
- 是否支持 "取最新 N 个" 的语法
- 是否同时支持 step_key 维度的过滤

## 状态

Parking — 等 context-binding-simplification 完成后再评估。

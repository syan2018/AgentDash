# 清理 record_artifacts 旧体系 + lifecycle_vfs 瘦身

## Goal

移除已废弃的 `WorkflowRecordArtifact` 整套体系。该体系的入口工具 (`report_workflow_artifact`) 已在之前的 port-gating task 中删除，剩余的类型定义、hook 自动生成、VFS 路径、block preset 均为死代码。清理后 lifecycle_vfs 仅保留 port_outputs + step_states + execution_log 三类有效数据。

## Background

- `04-15-agent-node-io-port-gating` AD7 已确认移除旧 artifact 体系
- `04-16-inline-fs-storage-refactor` R9 原计划将 record_artifacts content 迁移到 inline_fs — 现改为直接删除
- port_outputs 已通过 inline_fs_files 表独立存储，record_artifacts 是唯一残留的非标准化嵌套储存

## Scope

### 删除项

**Domain:**
- `WorkflowRecordArtifactType` enum
- `WorkflowRecordArtifact` struct
- `WorkflowCompletionSpec.default_artifact_type` / `default_artifact_title`
- `LifecycleRun.record_artifacts` 字段
- `LifecycleRun::append_record_artifact()` 方法

**Application:**
- `WorkflowRecordArtifactDraft` struct + `build_step_completion_artifact_drafts()`
- `advance_node.rs` 中的 artifacts 参数解析
- `completion.rs` 中的 `build_completion_record_artifacts_from_snapshot()` + 调用方
- `provider_lifecycle.rs` 中所有 `active/artifacts/*` 和 `nodes/*/artifacts/*` VFS 路径
- `block_record_artifact.rhai` 文件
- `rules.rs` / `presets.rs` 中的 block_record_artifact preset 注册
- `script_engine.rs` 中的 denied_artifact_types 逻辑

**Infrastructure:**
- `workflow_repository.rs` 中 record_artifacts 序列化/反序列化

**API:**
- `routes/workflows.rs` 中 artifact 相关 DTO

**Frontend:**
- `types/workflow.ts` 中 WorkflowRecordArtifact / WorkflowRecordArtifactType
- `task-workflow-panel.tsx` 中 CategorizedArtifacts 展示
- `workflowStore.ts` 中 record_artifacts 参数
- `services/workflow.ts` 中 artifact 映射

### 不删除

- `LifecycleRun.port_outputs` — 已迁移到 inline_fs，保留
- `LifecycleRun.execution_log` — 有效数据，保留
- `LifecycleRun.step_states` — 有效数据，保留
- `WorkflowCompletionSpec.checks` — 有效字段，保留

## Acceptance Criteria

- [ ] 编译通过 (cargo build)
- [ ] 前端构建通过 (npm run build)
- [ ] lifecycle_vfs 的 port_outputs 读写不受影响
- [ ] 所有 record_artifacts 相关类型和路径已清理

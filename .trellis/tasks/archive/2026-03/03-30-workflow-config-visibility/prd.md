# Workflow 配置系统清理：删除冗余层、类型化 SPI

## 背景

上一轮审查（commit `7300b14`）修复了前后端字段命名漂移、幽灵枚举、死类型等表层问题。
本轮聚焦后端 Rust 侧的结构性冗余和错误抽象。

## 决策记录

| 问题 | 方案 | 选择理由 |
|------|------|----------|
| DTO 层 ~394 行纯字段复制 | 激进删除，直接序列化 domain 类型 | domain 已 derive Serialize；DTO 唯一差异是剥离字段，而那些字段恰好是前端需要的 |
| ActiveWorkflowMeta 20 个 Option\<String\> | SPI 已依赖 domain，直接用 domain VO 替换 | 消除 JSON round-trip，获得编译期类型安全 |
| step_advance + transition_policy 重复 | 合并为单一字段 transition_policy | 两字段始终写入相同值 |
| Constraint 特殊化执行逻辑 | **保留** — 作为预设 hook 行为 | 后续在前端统一按 trigger 展示可选预设，再支持自定义脚本扩展 |
| 前端注入预览/结构化 check 编辑器 | 独立任务跟踪 | 需要先暴露 hook rule registry 到 API |

## 已完成修复

### 1. 合并 step_advance / transition_policy

- `ActiveWorkflowMeta`: 删除 `step_advance`，保留 `transition_policy`
- `provider.rs`: 只写 `transition_policy`
- `snapshot_helpers.rs`: `workflow_transition_policy()` 只读 `transition_policy`
- `test_fixtures.rs`: 统一使用 `transition_policy`

### 2. ActiveWorkflowMeta 类型化

将扁平 `Option<String>` 字段替换为 domain VO 类型：
- `lifecycle_id` / `run_id` / `primary_workflow_id`: `Option<Uuid>`
- `run_status`: `Option<LifecycleRunStatus>`
- `effective_contract`: `Option<EffectiveSessionContract>`（消除 JSON round-trip）
- `default_artifact_type` / `checklist_evidence_artifact_type`: `Option<WorkflowRecordArtifactType>`
- `checklist_evidence_artifact_ids`: `Option<Vec<Uuid>>`

### 3. DTO 层激进简化

- 删除全部 1:1 拷贝 Response struct + From impl（~350 行）
- 只保留 `WorkflowValidationResponse`（聚合型，非 1:1 映射）
- 顶层 entity（`WorkflowDefinition`, `LifecycleDefinition`, `LifecycleRun`, `WorkflowAssignment`）直接序列化
- API 响应现在包含之前被 DTO 剥离的 `execution_log` 和 `context_snapshot`

### 4. 清理死代码

- 删除 `parse_workflow_record_artifact_type_tag()`（不再需要字符串→枚举手动解析）
- 删除 `workflow_record_artifact_type_tag()`（不再需要枚举→字符串手动映射）
- 修复 `before_tool_blocks_record_artifact_during_implement_phase` 测试（预先存在的 fixture 不匹配）

## 后续任务（独立跟踪）

- **前端 hook 预设逻辑展示 + 自定义扩展**：暴露 hook rule registry 到 API，前端按 trigger 分组展示可选预设行为，支持未来自定义 hook 脚本

## 验收标准

- [x] `cargo check` 编译通过
- [x] `cargo test` 全部 80 测试通过
- [x] API 响应中 `LifecycleRun` 包含 `execution_log`
- [x] API 响应中 `LifecycleStepState` 包含 `context_snapshot`
- [x] `ActiveWorkflowMeta.effective_contract` 使用 `EffectiveSessionContract` 类型
- [x] 静态 hook rules 保留完整
- [x] 前端 `npx tsc --noEmit` 通过

# 统一 Workflow Ports 为 Single Source of Truth

> 状态：已完成 | 完成日期：2026-04-24

## 背景

WorkflowContract 中存在三组重叠的声明：

- `completion.checks` — 完成条件（如 checklist_passed）
- `constraints` — 运行约束（如 session_terminal_in / explicit_action_received）
- `recommended_output_ports` / `recommended_input_ports` — 推荐端口

实际运行中，output ports 的 `gate_strategy` 已经覆盖了完成门禁的全部语义，input ports 也覆盖了"所需外部数据"这一约束子集。两组旧字段带来了前端硬编码、Rhai hook 逻辑重复、以及测试维护负担。

## 决策

| 项目 | 决策 |
|------|------|
| `completion.checks` | **废弃删除** — 由 `output_ports` + `port_output_gate.rhai` 取代 |
| `constraints` | **废弃删除** — 数据类约束由 `input_ports` 表达；非数据类门禁（session_terminal_in 等）下沉为 hook_rule preset |
| `recommended_output_ports` | **更名为 `output_ports`** — 去掉 recommended 前缀，成为一级字段 |
| `recommended_input_ports` | **更名为 `input_ports`** — 同上 |
| `standalone_fulfillment` | **新增** — InputPortDefinition 新增此字段，声明 workflow 独立运行时如何满足输入（TextInput / FileUpload / None） |
| `stop_gate_checks_pending.rhai` | **废弃** — `port_output_gate.rhai` 成为唯一的产出门禁 |

## 变更范围

### Domain (Rust)
- `WorkflowContract`: 移除 `constraints` / `completion` 字段，重命名 `recommended_*` → 直接字段
- `InputPortDefinition`: 添加 `standalone_fulfillment: StandaloneFulfillment`
- 新增 `StandaloneFulfillment` 枚举
- 旧字段名通过 `serde(alias)` 保持反序列化兼容

### Application (Rust)
- `snapshot_helpers.rs`: 删除 `workflow_auto_completion_snapshot` / `active_workflow_checklist_evidence` / `checklist_evidence_present` / `workflow_transition_policy` / `active_workflow_contract`
- `test_fixtures.rs`: `snapshot_with_workflow_and_evidence` → `snapshot_with_workflow_ports`
- `rules.rs`: 删除 4 个旧 completion 测试，新增 3 个 port_output_gate 测试
- `provider.rs`: 删除 constraint injection 代码块
- `catalog.rs` / `step_activation.rs`: 补全 `standalone_fulfillment` 字段

### SPI
- `ActiveWorkflowMeta`: 移除 `checklist_evidence_present` 字段

### MCP
- `workflow.rs`: `WorkflowContractInput` 移除 `constraints` / `completion`，重命名 port 字段

### Frontend
- `types/workflow.ts`: 删除 5 个废弃类型，新增 `StandaloneFulfillment`
- `services/workflow.ts`: 删除 3 个旧 mapper，更新 `mapWorkflowContract` 带兼容回退
- `workflow-editor.tsx`: 删除 `CompletionEditor` / `ConstraintListEditor`，重命名 `RecommendedPortsEditor` → `PortsEditor`
- `workflow-tab-view.tsx` / `WorkflowCategoryPanel.tsx`: 更新摘要统计
- `dag-side-panel.tsx` / `lifecycle-dag-editor.tsx`: 更新 port 引用路径

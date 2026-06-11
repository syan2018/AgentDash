# 🌸 屎山代码分析报告 🌸

## 📑 目录

- [糟糕指数](#overall-score)
- [评分指标详情](#metrics-details)
- [最屎代码排行榜](#problem-files)
- [诊断结论](#conclusion)

![Score](https://img.shields.io/badge/Score-70%25-green)

## 糟糕指数 {#overall-score}

| 指标摘要 | 评分 |
|------|-------|
| **糟糕指数** | **70.12/100** |
| 屎山等级 | 😷 屎气扑鼻 |

> 略带清香，偶尔飘过一丝酸爽

### 📊 统计信息

| 指标 | 数值 |
|--------|-------|
| 总文件数 | 5 |
| 已跳过 | 0 |
| 耗时 | 210ms |

## 评分指标详情 {#metrics-details}

| 指标摘要 | 评分 | 状态 |
|:-----|------:|:------:|
| 循环复杂度 | 13.40% | ✓✓ |
| 认知复杂度 | 14.64% | ✓✓ |
| 嵌套深度 | 25.00% | ✓ |
| 函数长度 | 8.22% | ✓✓ |
| 文件长度 | 64.48% | ⚠ |
| 参数数量 | 14.00% | ✓✓ |
| 代码重复 | 20.14% | ✓ |
| 结构分析 | 26.60% | ✓ |
| 错误处理 | 64.90% | ⚠ |
| 注释比例 | 100.00% | ✗ |
| 命名规范 | 0.00% | ✓✓ |

## 最屎代码排行榜 {#problem-files}

### 1. runtime.rs

**糟糕指数: 43.77**

**问题**: 🔄 复杂度问题: 11, ⚠️ 其他问题: 3, 📋 重复问题: 4, 🏗️ 结构问题: 7, ❌ 错误处理问题: 5, 📝 注释问题: 1

- 🔄 `apply_orchestration_event_inner()` L267: 复杂度: 15
- 🔄 `validate_completion_policy()` L408: 复杂度: 11
- 🔄 `sync_lifecycle_run_status_from_orchestrations()` L966: 复杂度: 11
- 🔄 `apply_orchestration_event_inner()` L267: 认知复杂度: 19
- 🔄 `validate_completion_policy()` L408: 认知复杂度: 17
- 🔍 ...还有 23 个问题实在太屎，列不完了

### 2. script_compiler.rs

**糟糕指数: 28.59**

**问题**: 🔄 复杂度问题: 3, ⚠️ 其他问题: 2, 📋 重复问题: 3, 🏗️ 结构问题: 7, ❌ 错误处理问题: 6, 📝 注释问题: 1

- 🔄 `compile_statement()` L459: 复杂度: 12
- 🔄 `compile_statement()` L459: 认知复杂度: 18
- 🔄 `compile_sequence()` L410: 嵌套深度: 4
- 📋 `root_arg_keys()` L1133: 重复模式: root_arg_keys, plan_digest, inline_agent_contract_digest, join_policy_label, script_compiler_maps_phase_pipeline_agent_function_human_gate, script_compiler_maps_local_effect_bash_and_capability, activation_materializes_root_args_into_entry_node_inputs
- 📋 `script_compiler_embeds_prompt_agent_as_snapshot_procedure()` L1602: 重复模式: script_compiler_embeds_prompt_agent_as_snapshot_procedure, phase_nodes_do_not_block_runtime_completion_when_activated
- 🔍 ...还有 14 个问题实在太屎，列不完了

### 3. compiler.rs

**糟糕指数: 27.39**

**问题**: 🔄 复杂度问题: 5, ⚠️ 其他问题: 3, 📋 重复问题: 4, 🏗️ 结构问题: 5, ❌ 错误处理问题: 3, 📝 注释问题: 1

- 🔄 `compile_artifact_bindings()` L551: 复杂度: 11
- 🔄 `compile_artifact_bindings()` L551: 认知复杂度: 19
- 🔄 `walk_cycles()` L723: 认知复杂度: 17
- 🔄 `compile_artifact_bindings()` L551: 嵌套深度: 4
- 🔄 `walk_cycles()` L723: 嵌套深度: 4
- 🔍 ...还有 14 个问题实在太屎，列不完了

### 4. executor_launcher.rs

**糟糕指数: 22.73**

**问题**: 🔄 复杂度问题: 2, ⚠️ 其他问题: 4, 🏗️ 结构问题: 4, ❌ 错误处理问题: 16, 📝 注释问题: 1

- 🔄 `drain_ready_nodes()` L119: 复杂度: 11
- 🔄 `drain_ready_nodes()` L119: 认知复杂度: 17
- 📏 `launch_agent_node()` L232: 129 代码量
- 📏 `execute_function_like_node()` L529: 108 代码量
- 🏗️ `drain_ready_nodes()` L119: 中等嵌套: 3
- 🔍 ...还有 19 个问题实在太屎，列不完了

### 5. mod.rs

**糟糕指数: 2.50**

**问题**: 📝 注释问题: 1

✓ 代码质量良好，没有明显问题

## 诊断结论 {#conclusion}

🌸 **屎气扑鼻** - 代码开始散发气味，谨慎维护

👍 继续保持，你是编码界的一股清流，代码洁癖者的骄傲

---

*由 [fuck-u-code](https://github.com/Done-0/fuck-u-code) 生成*
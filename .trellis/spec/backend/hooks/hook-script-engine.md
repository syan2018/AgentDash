# Hook Script Engine (Rhai)

> Hook 规则的嵌入式脚本执行引擎——将硬编码 Rust preset 函数迁移为可外置、可热注册的 Rhai 脚本。

---

## 概述

`HookScriptEngine` 位于 `agentdash-application-hooks`，负责预编译 builtin preset `.rhai` 脚本、运行时注册用户自定义 preset、沙箱求值并返回结构化 `ScriptDecision`。

Rhai 的具体执行能力由 `agentdash-infrastructure::script_runtime::RhaiScriptRuntime` 承载。该公共内核只管理 engine 初始化、sandbox limits、AST cache 和 `serde_json::Value` bridge；Hook adapter 负责注册 `block` / `inject` / `approve` 等 Hook helper，并维护 preset cache。这样 workflow script builder 等后续脚本入口可以复用同一 Rhai 安全内核，同时保持各自业务 surface 独立。

### 执行流程

```
WorkflowHookRuleSpec (domain 声明)
        ↓
apply_hook_rules() — Phase 1: global 硬编码规则
        ↓
apply_hook_rules() — Phase 2: contract-driven 规则
        ↓
    ┌── preset key → HookScriptEngine.eval_preset()
    └── inline script → HookScriptEngine.eval_script()
        ↓
    ScriptDecision → merge → HookResolution
```

- **Global 规则**（如 `shell_exec_absolute_cwd_rewrite`）保留为 Rust 硬编码——基础设施级，不可用户配置
- **Contract-driven 规则**（由 `WorkflowHookRuleSpec` 声明）全部通过脚本引擎执行

---

## 脚本上下文契约（`ctx` 对象）

| Key | 说明 |
| --- | --- |
| `trigger` / `tool_name` / `tool_call_id` | 触发信息 |
| `hook_target` | Hook 控制目标；存在 frame query 时携带 run / agent / frame / assignment refs，脚本需要判断业务 owner 时优先使用该对象 |
| `provenance` | runtime adapter 来源信息，包含可选 runtime session、turn 与 source，用于 trace / audit |
| `turn_id` / `session_id` | provenance alias，服务已有脚本的 trace 读取；它们不表达 Hook 控制 owner |
| `snapshot` | Session Snapshot 切片（owners / tags / injections） |
| `workflow` | Workflow 元数据（lifecycle_key / activity_key / transition_policy 等） |
| `contract` | Contract 切片（hook_rules / constraints / checks） |
| `meta` | Session 元数据（permission_policy / workspace_root 等） |
| `params` | `WorkflowHookRuleSpec.params` 透传 |
| `signals` | Rust 侧预计算的便利信号（避免脚本重复实现 snapshot 查询） |

> 完整字段定义见 `hooks::script_engine.rs` 的 `build_ctx_value()`。

关键约定：
- 不存在的字段值为 `()`（Rhai unit），脚本用 `== ()` 判空
- 复杂 snapshot 查询预计算为 `signals`，脚本只读信号

## 脚本返回值契约

脚本返回 Rhai map (`#{ ... }`)，字段全部可选：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `block` | `string` | 阻止当前操作 |
| `inject` | `array<{ slot, content, source }>` | 注入列表 |
| `approval` | `{ reason, details? }` | 请求用户审批 |
| `completion` | `{ mode, satisfied, reason }` | 完成状态信号 |
| `refresh` | `bool` | 请求刷新 snapshot |
| `rewrite_input` | `any` | 改写工具输入 |
| `diagnostics` | `array<{ code, message }>` | 诊断条目 |

返回 `#{}` 表示不匹配/无操作。

## 辅助函数

| 函数 | 签名 | 用途 |
| --- | --- | --- |
| `make_injection` | `(slot, content, source) -> Map` | 构造注入条目 |
| `make_diagnostic` | `(code, message) -> Map` | 构造诊断条目 |
| `block` | `(reason) -> Map` | 快捷阻止（返回完整决策 map） |
| `inject` | `(slot, content, source) -> Map` | 快捷注入 |
| `approve` | `(reason) -> Map` | 快捷审批请求 |
| `complete` | `(mode, satisfied, reason) -> Map` | 快捷完成信号 |
| `log` | `(message) -> Map` | 快捷诊断日志 |
| `requires_supervised_approval` | `(name) -> bool` | 判断是否需要监管审批 |

## 沙箱限制

| 限制项 | 值 | 说明 |
| --- | --- | --- |
| `max_operations` | 10,000 | 防止无限循环 |
| `max_call_levels` | 32 | 防止深递归 |
| `max_string_size` | 1 MB | 防止内存爆炸 |
| `max_array_size` | 1,000 | 防止大数组 |
| `max_map_size` | 500 | 防止大 map |

脚本无法访问文件系统、网络、进程等系统资源。

---

## 新增 Builtin Preset 步骤

1. 在 `scripts/hook-presets/` 下创建 `.rhai` 文件
2. 在 `presets.rs` 的 `PRESET_REGISTRY` 中添加 `HookRulePreset` 条目（`include_str!`）
3. 无需修改 `rules.rs`——引擎自动编译并注册
4. 在 workflow definition 的 `hook_rules` 中引用 `preset: "my_new_preset"`

## Builtin Preset Baseline

| Key | Trigger | 功能 |
| --- | --- | --- |
| `block_record_artifact` | `BeforeTool` | 禁止上报特定类型的 workflow artifact |
| `stop_gate_checks_pending` | `BeforeStop` | 完成条件门禁 |
| `manual_step_notice` | `BeforeStop` | 通知 Agent 当前 step 使用手动推进 |
| `context_compaction_trigger` | `AfterCompaction` | 压缩后刷新 snapshot |
| `subagent_inherit_context` | `BeforeSubagentDispatch` | 子 Agent 继承注入和约束 |
| `subagent_record_result` | `AfterSubagentDispatch` | 记录子 Agent 派发结果 |
| `companion_result_channel` | `CompanionResult` | 处理 Companion 回流 |
| `supervised_tool_gate` | `BeforeTool` | SUPERVISED 策略工具审批门禁 |

---

## 为什么选择 Rhai

与 Rust 类型系统天然互操作（`rhai::serde`），内建沙箱限制，语法对前后端开发者友好，编译后 AST 可缓存复用。备选项 Lua（FFI 复杂）、WASM（调试门槛高）、JSON DSL（表达力不足）均不如 Rhai 适合短小规则脚本场景。

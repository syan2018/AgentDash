# Hook Script Engine (Rhai)

> Hook 规则的嵌入式脚本执行引擎——将硬编码 Rust preset 函数迁移为可外置、可热注册的 Rhai 脚本。

---

## Overview

`HookScriptEngine` 是 `agentdash-application::hooks` 模块的核心组件，负责：

1. **预编译** 所有 builtin preset `.rhai` 脚本（启动时一次性编译为 AST）
2. **运行时注册** 用户自定义 preset 脚本
3. **沙箱求值** 每条 hook rule 的脚本，返回结构化 `ScriptDecision`
4. **AST 缓存** 自定义脚本按内容 hash 缓存，避免重复编译

该引擎取代了原先 `rules.rs` 中的 `apply_preset_rule` 硬编码 match 分发和 `legacy_workflow_hook_rule_registry`，使所有 workflow-driven hook 规则统一通过脚本执行。

### 与 Execution Hook Runtime 的关系

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
    ScriptDecision
        ↓
    merge_script_decision() → HookResolution
```

- **Global 规则**（如 `shell_exec_absolute_cwd_rewrite`、`supervised_tool_approval`、`after_tool_refresh`）仍保留为 Rust 硬编码，因为它们是基础设施级别的、不可由用户配置的行为
- **Contract-driven 规则**（由 `WorkflowHookRuleSpec` 声明）全部通过 `HookScriptEngine` 执行

---

## Scenario: Hook Script Engine 执行契约

### 1. Scope / Trigger

- Trigger: 新增或修改 workflow contract 中的 `hook_rules` 声明
- Trigger: 编写新的 builtin preset `.rhai` 脚本
- Trigger: 用户通过 API 注册自定义 hook 脚本
- Trigger: 需要理解脚本如何访问上下文、如何返回决策

### 2. Signatures

#### Domain 层 — WorkflowHookRuleSpec

```rust
pub struct WorkflowHookRuleSpec {
    pub key: String,
    pub trigger: WorkflowHookTrigger,
    pub description: String,
    pub preset: Option<String>,      // 引用 builtin/registered preset key
    pub params: Option<Value>,       // 传递给脚本的参数
    pub script: Option<String>,      // 内联 Rhai 脚本（与 preset 二选一）
    pub enabled: bool,               // 默认 true
}
```

**规则分发逻辑**：

- `preset` 有值 → `HookScriptEngine::eval_preset(preset_key, ctx, params)`
- `script` 有值 → `HookScriptEngine::eval_script(script, ctx, params)`
- 两者都无 → 跳过该规则

#### Application 层 — HookScriptEngine

```rust
pub(crate) struct HookScriptEngine {
    engine: Engine,
    ast_cache: RwLock<HashMap<u64, AST>>,       // 自定义脚本 hash → AST
    preset_asts: RwLock<HashMap<String, AST>>,   // preset key → AST
}

impl HookScriptEngine {
    pub fn new(preset_scripts: &[(&str, &str)]) -> Self;
    pub fn eval_preset(&self, key: &str, ctx: &HookEvaluationContext, params: Option<&Value>) -> Result<ScriptDecision, String>;
    pub fn eval_script(&self, script: &str, ctx: &HookEvaluationContext, params: Option<&Value>) -> Result<ScriptDecision, String>;
    pub fn validate_script(&self, script: &str) -> Result<(), Vec<String>>;
    pub fn register_preset(&self, key: &str, script: &str) -> Result<(), String>;
    pub fn remove_preset(&self, key: &str) -> bool;
}
```

#### Application 层 — ScriptDecision

```rust
pub(crate) struct ScriptDecision {
    pub block: Option<String>,
    pub inject: Vec<HookInjection>,
    pub approval: Option<HookApprovalRequest>,
    pub completion: Option<HookCompletionStatus>,
    pub refresh: bool,
    pub rewrite_input: Option<serde_json::Value>,
    pub diagnostics: Vec<HookDiagnosticEntry>,
}
```

#### API 层 — 管理端点


| 方法       | 路径                           | 功能                                  |
| -------- | ---------------------------- | ----------------------------------- |
| `POST`   | `/hook-scripts/validate`     | 验证 Rhai 脚本语法                        |
| `POST`   | `/hook-presets/custom`       | 注册自定义 preset                        |
| `DELETE` | `/hook-presets/custom/{key}` | 删除自定义 preset                        |
| `GET`    | `/hook-presets`              | 列出所有 preset（含 script 源码和 source 标记） |


#### Preset 注册表

```rust
pub struct HookRulePreset {
    pub key: &'static str,
    pub trigger: WorkflowHookTrigger,
    pub label: &'static str,
    pub description: &'static str,
    pub param_schema: Option<serde_json::Value>,
    pub script: &'static str,           // Rhai 脚本源码
    pub source: PresetSource,           // Builtin | UserDefined
}
```

### 3. Contracts

#### 3.1 脚本上下文契约 (`ctx` 对象)

每个 Rhai 脚本收到 `ctx` 变量，顶层 key：


| Key                                                                 | 说明                                                                                                                           |
| ------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `trigger` / `tool_name` / `tool_call_id` / `turn_id` / `session_id` | 触发信息                                                                                                                         |
| `snapshot`                                                          | Session Snapshot 切片（owners / tags / injections）                                                                              |
| `workflow`                                                          | Workflow 元数据（lifecycle_key / step_key / transition_policy / output_port_keys / fulfilled_port_keys / gate_collision_count 等） |
| `contract`                                                          | Contract 切片（hook_rules / constraints / checks）                                                                               |
| `meta`                                                              | Session 元数据（permission_policy / workspace_root 等）                                                                            |
| `params`                                                            | `WorkflowHookRuleSpec.params` 透传                                                                                             |
| `signals`                                                           | Rust 侧预计算的便利信号（避免脚本重复实现 snapshot 查询）                                                                                         |


> 完整字段定义见 `application::hooks::provider.rs` 中的 `build_script_context()`。

**关键约定**：

- 不存在的字段值为 `()`（Rhai 的 unit 类型），脚本中用 `== ()` 判空
- `signals` 是 Rust 侧预计算的便利信号，避免脚本重复实现复杂的 snapshot 查询逻辑

#### 3.2 脚本返回值契约

脚本必须返回一个 Rhai map (`#{ ... }`)，字段全部可选：


| 字段              | 类型                                 | 说明              |
| --------------- | ---------------------------------- | --------------- |
| `block`         | `string`                           | 阻止当前操作的原因文本     |
| `inject`        | `array<{ slot, content, source }>` | 注入列表            |
| `approval`      | `{ reason, details? }`             | 请求用户审批          |
| `completion`    | `{ mode, satisfied, reason }`      | 完成状态信号          |
| `refresh`       | `bool`                             | 是否请求刷新 snapshot |
| `rewrite_input` | `any`                              | 改写工具输入          |
| `diagnostics`   | `array<{ code, message }>`         | 诊断条目            |


**空决策**：返回 `#{}` 表示该规则不匹配/无操作，不会被记录到 `matched_rule_keys`。

**非空决策**：只要返回的 map 中有任何有效字段，就会被视为匹配，记录到 `matched_rule_keys` 并 merge 进 `HookResolution`。

#### 3.3 辅助函数契约

引擎注册了以下 Rhai 全局函数，脚本可直接调用：


| 函数                             | 签名                                 | 返回值                         | 用途                  |
| ------------------------------ | ---------------------------------- | --------------------------- | ------------------- |
| `make_injection`               | `(slot, content, source) -> Map`   | `{ slot, content, source }` | 构造注入条目              |
| `make_diagnostic`              | `(code, message) -> Map`           | `{ code, message }`         | 构造诊断条目              |
| `block`                        | `(reason) -> Map`                  | `{ block: reason }`         | 快捷阻止                |
| `inject`                       | `(slot, content, source) -> Map`   | `{ inject: [...] }`         | 快捷注入（单条）            |
| `approve`                      | `(reason) -> Map`                  | `{ approval: { reason } }`  | 快捷审批请求              |
| `complete`                     | `(mode, satisfied, reason) -> Map` | `{ completion: { ... } }`   | 快捷完成信号              |
| `log`                          | `(message) -> Map`                 | `{ diagnostics: [...] }`    | 快捷诊断日志              |
| `requires_supervised_approval` | `(name) -> bool`                   | —                           | 判断是否需要监管审批          |


**快捷函数 vs 完整 map**：

- 快捷函数（`block`、`inject`、`approve`、`complete`、`log`）返回的是完整的决策 map，可直接作为脚本返回值
- `make_injection`、`make_diagnostic` 返回的是条目对象，需要嵌入到 `inject` / `diagnostics` 数组中

#### 3.4 沙箱安全契约


| 限制项               | 值      | 说明      |
| ----------------- | ------ | ------- |
| `max_operations`  | 10,000 | 防止无限循环  |
| `max_call_levels` | 32     | 防止深递归   |
| `max_string_size` | 1 MB   | 防止内存爆炸  |
| `max_array_size`  | 1,000  | 防止大数组   |
| `max_map_size`    | 500    | 防止大 map |


脚本无法访问文件系统、网络、进程等系统资源。

#### 3.5 Preset 来源契约

```rust
pub enum PresetSource {
    Builtin,       // 随代码发布，include_str! 编译进二进制
    UserDefined,   // 运行时通过 API 注册
}
```

- Builtin preset 不可通过 `remove_preset` 删除（API 层应限制）
- UserDefined preset 可随时注册/覆盖/删除
- 前端通过 `source` 字段区分展示样式

### 4. Validation & Error Matrix


| 场景                    | 预期行为                       | 错误/结果                                |
| --------------------- | -------------------------- | ------------------------------------ |
| 脚本语法错误（编译期）           | `validate_script` 返回错误列表   | `Err(vec!["..."])`                   |
| preset 脚本编译失败（启动时）    | 日志 error，该 preset 不注册      | 运行时 `eval_preset` 返回"未知 preset"      |
| 脚本运行时错误（除零、类型错误等）     | 返回 `Err(String)`           | 写入 diagnostics `hook_script_error`   |
| 脚本超过 `max_operations` | Rhai 引擎中断                  | 返回 `Err("...")`                      |
| 脚本返回非 map 值（如 `42`）   | 视为空决策                      | `ScriptDecision::is_empty() == true` |
| 脚本返回空 map `#{}`       | 视为不匹配                      | 不记录到 `matched_rule_keys`             |
| 脚本返回 `()` (unit)      | 视为空决策                      | 同上                                   |
| 引用未知 preset key       | 返回 `Err("未知 preset: xxx")` | 写入 diagnostics                       |
| 自定义脚本首次执行             | 编译并缓存 AST                  | 后续执行直接使用缓存                           |
| 注册同名 preset           | 覆盖已有 AST                   | 新脚本立即生效                              |


### 5. Good / Base / Bad Cases

#### Good

```text
workflow contract 声明 hook_rules:
  - key: "block_artifact", preset: "block_record_artifact", params: { artifact_types: ["session_summary"] }
        ↓
apply_hook_rules → apply_contract_hook_rules
        ↓
HookScriptEngine.eval_preset("block_record_artifact", ctx, params)
        ↓
block_record_artifact.rhai 读取 ctx.params.artifact_types，匹配 ctx.tool_name
        ↓
返回 #{ block: "...", diagnostics: [...] }
        ↓
merge_script_decision → HookResolution.block_reason = Some("...")
```

#### Base

```text
workflow contract 声明 hook_rules:
  - key: "custom_gate", script: "if ctx.tool_name == \"shell_exec\" { block(\"no shell\") } else { #{} }"
        ↓
HookScriptEngine.eval_script(inline_script, ctx, None)
        ↓
编译 → 缓存 AST → 执行 → 返回 ScriptDecision
```

#### Bad

```text
在 rules.rs 的 apply_contract_hook_rules 中为新 preset 添加硬编码 match arm
在脚本中直接查询数据库或文件系统
在脚本中构造超大字符串绕过沙箱限制
把 global 基础设施规则（如 cwd rewrite）迁移到脚本中
```

### 6. Tests Required

#### script_engine 单测（已实现）

- `empty_script_returns_empty_decision` — `#{}` 返回空决策
- `script_can_block` — `#{ block: "..." }` 正确解析
- `script_can_inject` — `make_injection()` 正确构造注入
- `script_reads_ctx_trigger` — 脚本可读取 `ctx.trigger`
- `script_reads_ctx_params` — 脚本可读取 `ctx.params.*`
- `validate_catches_syntax_error` — 语法错误被捕获
- `validate_accepts_good_script` — 合法脚本通过验证
- `preset_registration_and_eval` — preset 注册后可执行
- `shortcut_block/inject/approve/complete/log` — 快捷函数正确返回

#### rules 集成测试（已更新）

- `before_tool_blocks_record_artifact_during_implement_phase` — 通过 preset 脚本阻止 artifact
- `before_stop_requires_checklist_evidence_when_auto_checks_enabled` — 通过 preset 脚本注入 stop gate
- `before_subagent_dispatch_inherits_runtime_context` — 通过 preset 脚本继承上下文
- `subagent_result_records_structured_return_channel_diagnostic` — 通过 preset 脚本处理回流

#### 编译检查

- `cargo check` 全 workspace 通过
- `cargo test -p agentdash-application -- hooks` 全部通过

### 7. Wrong vs Correct

#### Wrong

```text
为每个新 preset 在 rules.rs 中添加 match arm + 专用 rule_matches_xxx / rule_apply_xxx 函数对。
脚本逻辑和 Rust 逻辑并存，同一个 preset 既有 .rhai 文件又有 Rust 函数。
在脚本中重新实现 snapshot 查询逻辑（如遍历 constraints 判断 BlockStopUntilChecksPass）。
```

#### Correct

```text
所有 contract-driven 规则统一通过 HookScriptEngine 执行。
复杂的 snapshot 查询在 Rust 侧预计算为 ctx.signals，脚本只读取信号。
新增 preset 只需：
  1. 在 scripts/hook-presets/ 下新建 .rhai 文件
  2. 在 presets.rs 的 PRESET_REGISTRY 中添加条目（include_str!）
  3. 无需修改 rules.rs
```

---

## 编写 Rhai Hook 脚本指南

### 脚本文件位置

```
crates/agentdash-application/scripts/hook-presets/
├── block_record_artifact.rhai
├── session_terminal_advance.rhai
├── stop_gate_checks_pending.rhai
├── manual_step_notice.rhai
├── subagent_inherit_context.rhai
├── subagent_record_result.rhai
├── subagent_result_channel.rhai
└── supervised_tool_gate.rhai
```

### 脚本编写模式

#### 模式 1：条件阻止

```rhai
// 判断条件，不满足则返回空 map
if !some_condition(ctx) {
    return #{};
}

// 满足条件，返回阻止决策
#{
    block: "阻止原因文本",
    diagnostics: [
        make_diagnostic("diagnostic_code", "诊断消息")
    ]
}
```

#### 模式 2：条件注入

```rhai
if !should_inject(ctx) {
    return #{};
}

let src = ctx.workflow.source;

#{
    inject: [
        make_injection("workflow", "注入到 workflow slot 的内容", src),
        make_injection("constraint", "注入到 constraint slot 的约束", src)
    ],
    diagnostics: [
        make_diagnostic("injected_something", "已注入 XXX")
    ]
}
```

#### 模式 3：完成信号

```rhai
if ctx.workflow.transition_policy != "session_terminal_matches" {
    return #{};
}

#{
    completion: #{
        mode: "session_terminal_matches",
        satisfied: false,
        reason: "等待 session 进入终态"
    },
    diagnostics: [
        make_diagnostic("completion_signal", "已设置完成信号")
    ]
}
```

#### 模式 4：审批请求

```rhai
let tool = ctx.tool_name;
if tool == () || !requires_supervised_approval(tool) {
    return #{};
}

#{
    approval: #{
        reason: "执行 `" + tool + "` 需要用户审批",
        details: #{
            tool_name: tool,
            policy: "supervised"
        }
    }
}
```

### 编写注意事项

1. **判空用 `== ()`**：Rhai 中不存在 `null`，不存在的字段值为 `()`
2. **字符串拼接用 `+`**：`"前缀" + variable + "后缀"`
3. **提前返回用 `return #{};`**：不匹配时返回空 map
4. **使用 `ctx.signals` 而非重新计算**：复杂的 snapshot 查询已预计算为信号
5. **使用辅助函数**：`make_injection`、`make_diagnostic` 等确保结构正确
6. **脚本头部写注释**：说明 preset 用途和 params 含义
7. **保持脚本简短**：单个脚本建议不超过 100 行，复杂逻辑应拆分为多个 preset

### 新增 Builtin Preset 步骤

1. 在 `scripts/hook-presets/` 下创建 `.rhai` 文件
2. 在 `presets.rs` 的 `PRESET_REGISTRY` 中添加 `HookRulePreset` 条目：
  ```rust
   HookRulePreset {
       key: "my_new_preset",
       trigger: WorkflowHookTrigger::BeforeTool,
       label: "我的新预设",
       description: "描述这个预设做什么",
       param_schema: None,  // 或 Some(serde_json::json!({ ... }))
       script: include_str!("../../scripts/hook-presets/my_new_preset.rhai"),
       source: PresetSource::Builtin,
   },
  ```
3. 无需修改 `rules.rs`——引擎会自动编译并注册
4. 在相关 workflow definition 的 `hook_rules` 中引用 `preset: "my_new_preset"`

### 当前 Builtin Preset 清单


| Key                          | Trigger                  | 功能                                | 参数                         |
| ---------------------------- | ------------------------ | --------------------------------- | -------------------------- |
| `block_record_artifact`      | `BeforeTool`             | 禁止上报特定类型的 workflow artifact       | `artifact_types: string[]` |
| `session_terminal_advance`   | `BeforeStop`             | Session 终态自动推进 lifecycle step     | 无                          |
| `stop_gate_checks_pending`   | `BeforeStop`             | 完成条件门禁（checks 未满足时阻止结束）           | 无                          |
| `manual_step_notice`         | `BeforeStop`             | 通知 Agent 当前 step 使用手动推进           | 无                          |
| `task_session_terminal`      | `SessionTerminal`        | Task session 终态处理（状态回写 + step 推进） | 无                          |
| `context_compaction_trigger` | `AfterCompaction`        | 上下文压缩触发后刷新 snapshot               | 无                          |
| `subagent_inherit_context`   | `BeforeSubagentDispatch` | 子 Agent 继承当前 session 注入和约束        | 无                          |
| `subagent_record_result`     | `AfterSubagentDispatch`  | 记录子 Agent 派发结果                    | 无                          |
| `subagent_result_channel`    | `SubagentResult`         | 处理子 Agent 回流，按 adoption_mode 注入   | 无                          |
| `supervised_tool_gate`       | `BeforeTool`             | SUPERVISED 策略下工具审批门禁              | `allowlist?: string[]`     |


---

## Design Decision: 为什么选择 Rhai

**Context**: Hook 规则需要从硬编码 Rust 函数迁移到可配置的脚本，以支持用户自定义行为。

**Options Considered**:

1. **Lua (mlua)** — 生态成熟，但 FFI 边界复杂，沙箱配置不如 Rhai 原生
2. **Rhai** — Rust 原生嵌入式脚本，语法类 Rust/JS，沙箱内建，`serde` 互操作良好
3. **WASM** — 隔离性最强，但编写/调试门槛高，不适合短小的规则脚本
4. **JSON DSL** — 最简单，但表达力不足，复杂条件需要大量嵌套

**Decision**: 选择 Rhai，因为：

- 与 Rust 类型系统天然互操作（`rhai::serde`）
- 内建沙箱（operations/call levels/memory 限制）
- 语法对前端/后端开发者都友好
- `sync` feature 支持跨线程共享 `Engine`
- 编译后的 AST 可缓存复用

---

*创建：2026-03-30 — Hook Script Engine Rhai 脚本支持方案*
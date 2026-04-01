# Hook 脚本引擎：Rhai 嵌入 + Preset 策略外置统一

## 背景

当前 hook 系统有两个核心问题：

1. **script 字段是空壳** — `WorkflowHookRuleSpec.script` 在域模型和前端编辑器中均已预留，但后端完全未实现。用户无法编写自定义 hook 逻辑。

2. **大量策略硬编码在 Rust 源码中** — 7 个 preset、3 条 global rule、完整的 completion 决策管线，全部以 Rust match arm 形式内联。修改任何策略行为都需要重新编译。其中相当一部分并非通用引擎机制，而是特定项目的业务策略。

## 目标

引入 Rhai 脚本引擎，实现两个目标：

1. **打通 script 字段** — 用户可在前端编辑器中编写 Rhai 脚本作为 hook 规则
2. **Preset 外置** — 将现有 Rust 硬编码的 preset 实现迁移为 Rhai 脚本，与用户自定义脚本使用同一执行路径，统一 preset 和 script 的运行时模型

## 核心设计决策

### 为什么选 Rhai

| 维度 | Rhai | QuickJS | Relay 到本机执行 |
|------|------|---------|----------------|
| 桥接成本 | 极低，直接注册 Rust fn/struct | 需手写 JS↔Rust Value 转换 | 需序列化全部上下文通过 WebSocket |
| 沙箱控制 | 内建：禁 I/O、限步数/内存/递归 | 需手动删除危险 API | 依赖本机 OS 沙箱 |
| 上下文访问 | 零成本，直接访问 Rust 结构体 | 需序列化为 JS 对象 | 需完整序列化通过网络 |
| 依赖体积 | ~1MB，纯 Rust，无 C 依赖 | ~2MB，C 绑定 | 无新依赖但需本机在线 |
| 用户心智 | 类 JS/Rust 语法，需少量学习 | 标准 JS | 任意语言 |
| 编译影响 | 增量编译友好 | 需编译 C 代码 | 无 |

**决定性因素**：hook 决策需要的上下文（task 状态、workflow 进度、session 执行信息）全在云端内存中。Rhai 可以零成本访问这些 Rust 结构体，而任何"发到本机执行"的方案都需要序列化完整上下文走网络 round-trip。

**脚本定位**：hook 脚本不是通用编程，是**策略表达**——条件判断 + 返回决策对象。Rhai 的能力恰好覆盖这个范围，不多不少。

### Preset 与 Script 的统一模型

**核心思路**：preset 不再是 Rust match arm，而是**预装的 Rhai 脚本**。

```
用户自定义规则:  WorkflowHookRuleSpec { script: "if ctx.tool == ... { ... }" }
                     ↓
Preset 规则:     WorkflowHookRuleSpec { preset: "stop_gate_checks_pending" }
                     ↓ 查找 preset 注册表 → 得到一段 Rhai 脚本
                     ↓
              ┌──────────────────────────┐
              │   Rhai Engine.eval()     │  ← 统一执行入口
              │   输入: ctx 对象         │
              │   输出: HookDecision     │
              └──────────────────────────┘
```

**好处**：
- preset 变成了"官方提供的脚本模板"，用户可以 clone 后修改
- 新增/修改 preset 不需要重新编译
- preset 和 script 共享同一个 Rhai runtime、同一套 ctx API、同一个测试框架

## 需求清单

### P0：Rhai 引擎核心

#### R1: Rhai Engine 集成

- 在 `agentdash-application` 中引入 `rhai` crate
- 创建 `HookScriptEngine` 结构体，封装 Rhai `Engine` 实例
- 配置安全沙箱：
  - 禁用所有 I/O 包（无文件系统、无网络）
  - 设置最大执行步数（防死循环，建议 10_000）
  - 设置最大调用栈深度（建议 32）
  - 设置最大字符串长度（建议 1MB）
- Engine 实例在 `AppExecutionHookProvider` 初始化时创建，全生命周期复用

#### R2: 脚本上下文（ctx 对象）

将 `HookEvaluationQuery` + `SessionHookSnapshot` 映射为 Rhai 可访问的 `ctx` 对象：

```rhai
// ctx 提供的字段（只读）
ctx.trigger       // "before_tool" | "after_tool" | ...
ctx.tool_name     // Option<String>
ctx.tool_call_id  // Option<String>
ctx.subagent_type // Option<String>
ctx.turn_id       // Option<String>
ctx.session_id    // String
ctx.payload       // Dynamic (原始 JSON payload)

// ctx.snapshot — session 快照
ctx.snapshot.owners            // Array of { owner_type, owner_id, label, ... }
ctx.snapshot.tags              // Array of String
ctx.snapshot.injections        // Array of { slot, content, source }

// ctx.workflow — 活跃 workflow 元数据（如果有）
ctx.workflow.lifecycle_key     // Option<String>
ctx.workflow.step_key          // Option<String>
ctx.workflow.workflow_key      // Option<String>
ctx.workflow.transition_policy // Option<String>
ctx.workflow.run_status        // Option<String>
ctx.workflow.run_id            // Option<String>
ctx.workflow.checklist_evidence_present // Option<bool>

// ctx.contract — 当前生效的合约
ctx.contract.hook_rules        // Array
ctx.contract.constraints       // Array
ctx.contract.checks            // Array

// ctx.meta — session 运行时元数据
ctx.meta.permission_policy     // Option<String>
ctx.meta.working_directory     // Option<String>
ctx.meta.workspace_root        // Option<String>
ctx.meta.connector_id          // Option<String>
ctx.meta.executor              // Option<String>

// ctx.params — 规则级参数（来自 WorkflowHookRuleSpec.params）
ctx.params                     // Dynamic (规则配置的 params JSON)
```

实现方式：使用 Rhai 的 `CustomType` derive 或手动注册 getter，将 Rust 结构体字段暴露为只读属性。不需要序列化——Rhai 直接持有 Rust 引用。

#### R3: 脚本返回值（HookDecision）

脚本返回一个 Rhai map，引擎将其映射为 `HookResolution` 的增量修改：

```rhai
// 最简：无操作（等同于 preset 不匹配）
#{}

// 阻塞工具调用
#{ block: "文件过大，需审批" }

// 注入上下文
#{ inject: [#{ slot: "constraint", content: "请先完成 lint 检查", source: "custom:lint-gate" }] }

// 请求人工审批
#{ approval: #{ reason: "敏感操作需审批", details: #{} } }

// 标记 completion 状态
#{ completion: #{ mode: "auto", satisfied: true, reason: "所有检查通过" } }

// 刷新快照
#{ refresh: true }

// 重写工具输入
#{ rewrite_input: #{ command: "modified command" } }

// 组合多个效果
#{
    inject: [#{ slot: "workflow", content: "...", source: "..." }],
    completion: #{ mode: "session_terminal", satisfied: false, reason: "..." },
    refresh: true
}
```

引擎负责将这个 map 合并到 `HookResolution` 中。字段不存在即为 no-op。

#### R4: 规则引擎集成

修改 `apply_contract_hook_rules()`（rules.rs），当规则有 `script` 字段时走 Rhai 执行路径：

```rust
for rule in contract.hook_rules.iter().filter(|r| r.enabled) {
    if !trigger_matches(&rule.trigger, &query.trigger) { continue; }

    let decision = if let Some(preset_key) = &rule.preset {
        // 查找 preset 注册表 → 得到 Rhai 脚本 → 执行
        engine.eval_preset(preset_key, &ctx, &rule.params)?
    } else if let Some(script) = &rule.script {
        // 直接执行用户脚本
        engine.eval_script(script, &ctx, &rule.params)?
    } else {
        continue;
    };

    merge_decision_into_resolution(&mut resolution, decision, &rule.key);
}
```

注意：整个函数已经在 async 上下文中（`ExecutionHookProvider` trait 的方法是 async fn），Rhai 的 eval 本身是同步的，直接在 async 中调用即可（如果担心阻塞可以 `spawn_blocking`）。

### P0：Preset 外置迁移

#### R5: Preset 注册表重构

将现有 `presets.rs` 中的 7 个 `HookRulePreset` 描述符扩展，每个 preset 关联一段 Rhai 脚本：

```rust
pub struct HookRulePreset {
    pub key: String,
    pub label: String,
    pub description: String,
    pub trigger: WorkflowHookTrigger,
    pub default_params: Option<Value>,
    pub script: String,          // ← 新增：Rhai 脚本源码
    pub source: PresetSource,    // ← 新增：Builtin | UserDefined
}
```

Builtin preset 的 Rhai 脚本以 `include_str!` 方式嵌入编译产物，保证无外部文件依赖。脚本源码同时保存在仓库中（如 `scripts/hook-presets/`），方便审查和测试。

#### R6: 迁移现有 7 个 Preset

逐个将 `rules.rs` 中的 Rust match arm 翻译为等价的 Rhai 脚本：

| Preset | 当前实现位置 | 迁移难度 | 说明 |
|--------|-------------|---------|------|
| `block_record_artifact` | rules.rs 284-324 | 低 | 纯条件判断 |
| `session_terminal_advance` | rules.rs 326-370 | 低 | 条件 + 返回 completion |
| `stop_gate_checks_pending` | rules.rs 388-438 | 中 | 需要访问 completion 评估结果 |
| `manual_step_notice` | rules.rs 440-458 | 低 | 纯条件 + 返回 completion |
| `subagent_inherit_context` | rules.rs 460-481 | 低 | 拷贝 injections |
| `subagent_record_result` | rules.rs 483-525 | 低 | 提取 payload 字段 |
| `subagent_result_channel` | rules.rs 527-599 | 中 | 复杂 payload 解析 + 条件分支 |

#### R7: Global Rule 迁移评估

现有 3 条 global rule 的处理策略：

| Rule | 处理 |
|------|------|
| G1: `shell_exec_rewrite_absolute_cwd` | **保留为 Rust** — 这是路径安全机制，不应被用户脚本覆盖 |
| G2: `supervised_tool_approval` | **迁移为 preset** — 工具审批白名单应该可配置。新增 `supervised_tool_gate` preset，allowlist 通过 params 传入 |
| G3: `after_tool_refresh` | **保留为 Rust** — 快照刷新是引擎内部机制 |

#### R8: Legacy Rule 路径清理

迁移完成后：
- 删除 `legacy_workflow_hook_rule_registry()` 及所有 legacy match/apply 函数
- 删除 `entity.rs` 中的 `migrate_legacy_to_hook_rules()` 函数
- 所有 WorkflowContract 必须使用 hook_rules（如果旧数据没有，提供数据迁移脚本）

### P1：前端编辑器对接

#### R9: Script 编辑器升级

- 将 NewRuleEditor 的 script textarea 改为代码编辑器（Monaco 或 CodeMirror）
- 语法高亮：Rhai 语法接近 JS，可复用 JS/Rust 高亮规则
- 提供 ctx 对象的类型提示/自动补全（可选，P2）
- 移除"后续版本支持解释执行"的 placeholder 标注

#### R10: Preset 模板浏览与 Clone

- 用户选择 preset 后，可以"查看脚本"看到 Rhai 源码
- 提供"Clone 为自定义"操作——将 preset 的脚本复制到 script 字段，清除 preset 引用
- 前端 preset 列表改为从后端 `/hook-presets` 获取，后端返回包含 `script` 字段的完整 preset 描述

### P1：辅助能力

#### R11: 脚本验证 API

- 新增 API endpoint `POST /hook-script/validate`
- 接受 `{ script: string, trigger: string }`
- 返回 `{ valid: bool, errors: [...], warnings: [...] }`
- 验证内容：语法检查、返回值类型检查、已知 ctx 字段访问检查
- 前端在保存 workflow 前调用此 API 做预验证

#### R12: 脚本执行跟踪

- Rhai 脚本执行结果纳入现有 `HookTraceEntry` 体系
- trace 中记录：脚本 key、执行耗时、返回的 decision 摘要
- 脚本运行时错误记录到 `diagnostics`，不中断 hook 管线（优雅降级）

### P2：高级特性（后续迭代）

#### R13: Rhai 内置辅助函数库

为常见 hook 模式提供内置函数，减少脚本样板代码：

```rhai
// 内置辅助函数示例
fn inject(slot, content, source)     // 快捷构造 injection
fn block(reason)                      // 快捷构造 block decision
fn approve(reason)                    // 快捷构造 approval request
fn complete(mode, satisfied, reason)  // 快捷构造 completion
fn log(message)                       // 写入 diagnostics
```

#### R14: Preset 热更新

- Builtin preset 之外，支持在数据库中存储自定义 preset（UserDefined 类型）
- 管理员可通过 API 注册新的全局 preset
- preset 变更时自动使引用此 preset 的 workflow 获得新版本

## 技术约束

### 安全

- Rhai 脚本无文件系统、网络、进程访问能力
- 执行步数上限防死循环
- 单次 eval 超时保护（建议 100ms）
- 脚本错误不得 crash 宿主进程——返回默认空 HookResolution + 诊断条目

### 性能

- Rhai Engine 实例全局复用（线程安全，可跨 session 共享）
- Preset 脚本可预编译为 AST 缓存（`Engine::compile()`）
- 用户脚本首次执行时编译，后续缓存 AST（以 script hash 为 key）

### 兼容性

- 现有无 hook_rules 的 WorkflowContract 继续工作（空 hook_rules = 无脚本执行）
- 迁移期间保留 legacy 路径作为 fallback，全部 preset 迁移验证后再删除
- `HookResolution` 结构体不变，脚本输出映射到现有字段

## 验收标准

- [ ] `rhai` crate 集成，HookScriptEngine 通过安全沙箱配置
- [ ] ctx 对象完整暴露 snapshot/workflow/contract/meta 信息
- [ ] 用户 script 字段可执行 Rhai 脚本并产出 HookResolution
- [ ] 7 个 preset 全部迁移为 Rhai 脚本，通过等价性测试
- [ ] G2 (supervised_tool_approval) 迁移为可配置 preset
- [ ] Legacy rule 路径删除，无残留代码
- [ ] 前端 script 编辑器支持 Rhai 脚本输入
- [ ] 前端支持查看 preset 脚本源码和 clone 为自定义
- [ ] 脚本验证 API 可用
- [ ] 脚本执行错误优雅降级，不影响 session
- [ ] hook trace 包含脚本执行记录

## 实现顺序建议

```
Phase 1 (P0):  R1 → R2 → R3 → R4          Rhai 引擎核心 + script 字段打通
Phase 2 (P0):  R5 → R6 → R7               Preset 外置迁移
Phase 3 (P1):  R9 → R10 → R11 → R12       前端 + 辅助能力
Phase 4:       R8                           Legacy 清理（确认 Phase 2 稳定后）
Phase 5 (P2):  R13 → R14                   高级特性
```

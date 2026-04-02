# Hook Action 硬编码逻辑清理

## 背景

companion 信道统一后发现：`blocking_review` stop gate、`follow_up_required` 分流、pending action 指令文本等逻辑直接硬编码在工具和 hook_delegate 中，而不是由 workflow 规则驱动。这导致：

1. 工具 panic 时遗留 blocking_review 会造成无限 steer 循环
2. stop gate 行为不可配置、不可观测来源
3. action_type 的语义散落在多个文件的字符串比较中

## 审计结果

### 关键硬编码点

#### 1. `is_blocking()` — SPI 层硬编码

`crates/agentdash-spi/src/hooks.rs:204`

```rust
pub fn is_blocking(&self) -> bool {
    self.action_type == "blocking_review"
}
```

整个 stop gate 体系的根 — 单一字符串比较，不可配置。

#### 2. hook_delegate `before_stop` 决策逻辑

`crates/agentdash-application/src/session/hook_delegate.rs:342-414`

- `unresolved_blocking_actions()` 硬编码调 `is_blocking()`
- 硬编码 rule key `"runtime_pending_action:blocking_review:stop_gate"`
- 4 条 reason 常量硬编码在 `hook_messages.rs`
- stop gate 条件是 5 个硬编码布尔表达式的组合

#### 3. hook_delegate `collect_pending_hook_messages` 分流

`crates/agentdash-application/src/session/hook_delegate.rs:452`

```rust
if action.action_type == "follow_up_required" {
    messages.follow_up.push(message);
} else {
    messages.steering.push(message);
}
```

action_type 到消息路由的映射硬编码。

#### 4. `pending_action_instruction()` 文本模板

`crates/agentdash-application/src/session/hook_messages.rs:64-78`

- 硬编码 `"blocking_review"` / `"follow_up_required"` 的指令文本
- 直接引用工具名 `companion_respond`

#### 5. companion tools 中 action_type 创建

`crates/agentdash-application/src/companion/tools.rs`

- 行 421, 517, 587: 硬编码 `"follow_up_required"`
- 行 662-666: 条件硬编码 `"suggestion"` / `"follow_up_required"`
- `build_subagent_pending_action` 行 1379: 硬编码 `"suggestion"` 过滤

### 应该由 workflow 规则驱动的部分

| 当前硬编码 | 应有来源 |
|-----------|---------|
| `is_blocking()` 判据 | workflow contract 或 hook policy 声明 |
| before_stop 决策组合条件 | hook rule engine 评估结果 |
| action_type → 消息路由 | hook policy 的 action_type 配置 |
| pending action 指令文本 | workflow/hook 注入的 instruction fragment |
| companion 工具的 action_type | 调用方 payload 或 workflow 规则，而非工具硬编码 |

## 清理方向

### 短期（最小改动）

- [ ] `is_blocking()` 改为接受可配置的 blocking action_type 列表
- [ ] `pending_action_instruction()` 改为从 action 自身 injections 取指令，移除硬编码文本
- [ ] companion tools 的 action_type 由 payload.type → PayloadTypeRegistry 映射得出，不硬编码

### 中期（结构性改动）

- [ ] before_stop 的 blocking 判据从硬编码条件改为 hook rule evaluation 的输出
- [ ] action_type → 消息路由改为 hook policy 配置
- [ ] stop gate reason 从常量改为 hook resolution 携带的结构化信息

### 关联问题：start_prompt 混入 session 初始化

`SessionHub::start_prompt` 当前同时承担：
1. session 初始化（setup executor、build tools、注入 project instruction）
2. 注入用户 prompt 并启动 turn

这导致：
- 每次 `start_prompt` 都可能重跑 setup（重复注入 instruction）
- companion respond fallback 调 `start_prompt` 触发完整初始化（500 报错 / 重复注入）
- 用户输入不应该携带初始化副作用

应有架构：session 初始化由 session 自身生命周期管理（首次绑定 executor 时 setup 一次），`start_prompt` 只负责"向已 ready 的 session 注入 prompt"。

### 非目标

- 不改变 `blocking_review` 作为 adoption_mode 的语义（由调用方显式指定时是合理的）
- 不改变 workflow 规则引擎（Rhai）的执行模型
- 不改变 before_stop / after_turn 的 delegate 边界

## Acceptance Criteria

- [ ] 工具层零 `blocking_review` / `follow_up_required` 硬编码字符串
- [ ] `is_blocking()` 判据可配置
- [ ] pending action 指令文本不引用具体工具名
- [ ] action_type → 行为的映射从字符串比较改为注册表查找
- [ ] `start_prompt` 不混入 session 初始化逻辑

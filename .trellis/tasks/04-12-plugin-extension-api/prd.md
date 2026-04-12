# Plugin 扩展 API

> 状态：planning
> 来源：从 `04-08-hook-event-coverage` 拆出，原 PRD §5-7

## 背景

`04-08-hook-event-coverage` 任务原计划同时补齐 Hook 事件和 Plugin API 两个方向。
经讨论决定：Hook 事件补全已独立完成，Plugin API 属于不同层面的扩展系统，拆为独立任务。

参考来源：`references/pi-mono/packages/coding-agent/src/core/extensions/types.ts`

## 目标

为 AgentDash 构建 Plugin 扩展层，让外部 Plugin 可以：
1. 注册自定义 slash 命令
2. 注册运行时 flag（session 作用域）
3. 向对话注入自定义载荷消息

## 需求

### 1. registerCommand — Plugin 注册 Slash 命令

Plugin 可通过 trait 注册自定义 slash 命令，与 skill `/skill:` 命令共享同一套路由。

```rust
pub trait PluginCommandProvider {
    fn commands(&self) -> Vec<PluginCommand>;
}

pub struct PluginCommand {
    pub name: String,           // 不含斜杠，如 "my-plugin:reset"
    pub description: String,
    pub handler: PluginCommandHandler,
}

pub enum PluginCommandHandler {
    InjectMessage(String),          // 直接注入固定消息
    TriggerHook(HookTrigger),       // 触发指定 hook
}
```

### 2. registerFlag — Plugin 注册运行时 Flag

Flag 在 session 生命周期内存活，可被 Hook 规则（Rhai 脚本）读取。

```rust
pub struct PluginFlag {
    pub name: String,                   // 如 "my-plugin.verbose"
    pub flag_type: PluginFlagType,      // Bool | String
    pub default: serde_json::Value,
    pub description: String,
}
```

存储位置：可挂载到 `HookSessionState`（来自 `03-30-hook-external-triggers`）或独立的 flag store。

### 3. CustomMessage<T> — 通用 Extension 消息类型

在 `AgentMessage` 消息体系中新增 `role: "extension"` 类型：

```rust
pub struct ExtensionMessage {
    pub extension_id: String,
    pub custom_type: String,
    pub content: serde_json::Value,
    pub display: Option<String>,       // 可选的人类可读摘要
    pub exclude_from_context: bool,    // true 时不送给 LLM
}
```

前端通过 `custom_type` 路由到对应渲染组件。

## 依赖关系

- 可选依赖 `03-30-hook-external-triggers`（HookSessionState 作为 flag 存储）
- 需要 `agentdash-plugin-api` crate 已存在

## 验收标准

- [ ] Plugin 可注册 slash 命令，前端 `/` 菜单中可见
- [ ] Plugin 可注册 flag，Rhai 脚本中可通过 `flags.get("name")` 读取
- [ ] Extension message 可被注入对话，前端可按 custom_type 渲染
- [ ] 现有 first-party plugin 可作为示例使用新 API

## 技术备注

- 与 Hook 事件体系（HookTrigger）是不同层面的扩展机制
- Hook 是"agent 生命周期的拦截/注入点"，Plugin API 是"第三方注册能力"
- 两者可组合使用（Plugin 注册的命令可触发 Hook）

# 前端 Hook 预设逻辑展示 + 自定义扩展

## 背景

当前 workflow 介入工作流的核心逻辑是 hook 系统中的静态规则（`hook_rule_registry`），但这些规则对前端完全不可见。配置 workflow 时，用户无法知道哪些 hook 行为会在哪些 trigger 时机生效，只能靠手写 JSON。

## 目标

1. 将 hook rule registry 以只读方式暴露给前端
2. 前端按 `HookTrigger` 分组展示当前可用的预设 hook 行为
3. Workflow/Lifecycle 编辑器中，允许用户查看和关联预设 hook 行为到特定 step
4. 后续支持用户自定义 hook 脚本（解释执行），与预设行为并列展示

## 方案概要

### Phase 1: API 暴露 hook rule registry

- 新增 `GET /api/hooks/rules` 接口，返回静态规则列表
- 每条规则包含：`key`, `trigger`, `description`（可从规则 key 或注解中提取）
- 返回格式按 `trigger` 分组

### Phase 2: 前端展示

- Workflow 编辑页面增加 "Hook 行为" 面板
- 按 trigger 分组展示所有预设规则及其匹配条件的描述
- 读 only — 不支持修改预设规则

### Phase 3: 自定义 Hook 脚本

- 支持用户编写简单的 hook 脚本（DSL 或 JS/TS snippet）
- 脚本存储在 workflow 定义或独立的 hook_script 表中
- Hook runtime 在 evaluate 时同时执行静态规则和用户自定义脚本
- 前端编辑器支持创建/编辑自定义 hook 脚本

## 依赖

- `03-30-workflow-config-visibility` 中的 DTO 简化和 SPI 类型化（已完成）

## 开放问题

- 自定义脚本的沙箱执行策略（WASM? 受限 JS? Rhai?）
- 规则优先级：预设规则 vs 自定义脚本的冲突解决
- 是否需要"禁用预设规则"的能力

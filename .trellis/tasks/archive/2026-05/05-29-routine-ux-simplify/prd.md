# Routine 模块交互简化与心智模型优化

## Background

当前 Routine 创建流程要求用户做 4 层决策（触发类型 → 触发参数 → Session 策略 → Prompt 模板语法），心智模型过重，难以作为产品上手。参考 Multica Autopilot 的设计，其核心只有"谁 + 做什么 + 什么时候"三要素，Session 策略和模板语法完全不暴露给普通用户。

## Goal

将 Routine 创建交互从"配置一条规则"简化为"描述一个自动化任务"，降低认知负担，使非技术用户也能在 30 秒内创建一个可用的 Routine。

## Requirements

### P0 — 创建流程简化

1. **表单布局重构：左右分栏**
   - 左栏（主体）：名称、执行 Agent、指令描述（原 prompt_template 字段，重定位为 plain text）
   - 右栏（配置）：触发类型选择 + 对应配置（Schedule picker / Webhook info）
   - 去掉 Plugin 触发选项（仅保留后端 API 能力，不暴露在 UI）

2. **Session 策略降级为高级选项**
   - 默认使用 `fresh` 模式，创建表单中不展示
   - 移至 Routine 详情页 / 编辑弹窗的"高级设置"折叠区
   - `per_entity` 模式仅在触发类型为 webhook 时可见（scheduled 无意义）

3. **Prompt 模板交互优化**
   - 字段标签改为"执行指令"或"Runbook"
   - placeholder 改为自然语言示例而非模板语法
   - 模板变量提示移至折叠提示区（"支持模板变量 ›" 可展开查看）
   - 长期：payload 自动注入 agent 上下文，用户无需手写 `{{ }}` 语法

### P1 — 引导与发现性

4. **预设模板（Templates）**
   - 空状态页面提供 4-6 个预设模板卡片（如：每日代码审查、PR 自动 Review、定时报告生成、Webhook 事件响应）
   - 点击模板预填表单，用户只需选 Agent 即可创建
   - 模板数据前端硬编码即可（后续可扩展为后端配置）

5. **Webhook 创建后引导**
   - 创建成功后除了展示 token，增加"调用示例"代码块（curl 命令）
   - 展示完整 URL 而非仅 path

### P2 — 后续演进（不在本次范围）

- Trigger 独立实体化（1:N，一个 Routine 可挂多个 trigger）
- Plugin trigger UI 化（需要 plugin registry 支持）
- Prompt 模板的可视化编辑器
- Execution mode 概念引入（create_issue / run_only）

## Constraints

- 后端 API 保持兼容：数据模型不变，简化仅限前端交互层
- Session 策略能力不删除，仅从创建主流程隐藏
- Plugin 类型后端保留，前端创建入口移除

## Acceptance Criteria

- [ ] 创建表单字段数 ≤ 5（不含高级折叠区）
- [ ] 不阅读文档的用户能在 30 秒内完成一个 scheduled routine 的创建
- [ ] Session 策略不在创建主流程出现
- [ ] Plugin 触发类型不在 UI 可见
- [ ] 预设模板在空状态展示，点击可预填表单
- [ ] Webhook 创建后展示完整 curl 调用示例
- [ ] 现有功能无回归：已创建的 plugin / per_entity routine 在列表和编辑中正常展示

## References

- Multica Autopilot: `references/multica/packages/views/autopilots/components/autopilot-dialog.tsx`
- 当前实现: `packages/app-web/src/features/routine/routine-tab-view.tsx`
- Agent Preset Editor（项目内最新设计范式）: `packages/app-web/src/features/project/agent-preset-editor/`

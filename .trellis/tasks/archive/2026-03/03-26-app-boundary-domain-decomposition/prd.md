# 应用层边界收口与领域模型拆解重构

## Goal

完成当前 AgentDash 核心架构的一次完整收口，重点解决 application 层被 executor / relay / MCP 协议类型污染、领域/应用模型职责混杂、前端状态源重复、超大模块职责坍缩的问题。

本次工作不做“战术性减配”，而是以“输入输出明确、行为可验证、改完后更稳定”为约束推进。

## Scope

### 后端核心重构

- 为 `agentdash-application` 建立自有 runtime / session / address space 值模型
- 移除 `agentdash-application` 对 `agentdash-executor` / `agentdash-relay` 协议 DTO 的直接依赖
- 收口 application 与 API / executor 的边界转换职责
- 拆分高耦合的 application / api 模块，避免继续积累 God module
- 补充关键测试，保证输入输出形状稳定

### 前端状态重构

- 移除 `storyStore` 中的重复事实源，收口为单一数据源
- 修正受影响页面与选择器，保持会话页 / 看板页行为稳定

### 非目标

- 不在本次中引入新的产品能力
- 不做 UI 风格层面的改动
- 不为未上线项目添加兼容旧 DTO 的长期双轨逻辑

## Requirements

- [x] `agentdash-application` 中不再直接使用 `agentdash-relay::FileEntryRelay`
- [x] `agentdash-application` 中不再直接使用 `agentdash-executor::ExecutionAddressSpace / ExecutionMount / AgentDashExecutorConfig`
- [x] application 层核心函数签名改为 application 自有类型
- [x] API / executor 边界承担外部协议与 application 模型之间的转换
- [x] `storyStore` 收口为单一事实源，页面不再依赖重复缓存
- [x] 所有已完成重构必须配套定向测试或编译检查
- [x] 每个重构切片完成后更新 checklist

## Architecture Plan

### Phase 1: 建立 application 自有 runtime 模型

- 新建 application runtime/value 模块，承载：
  - executor config
  - thinking level
  - address space / mount / capability
  - address space list/read/exec 结果类型
  - MCP server 抽象或最小化表达

### Phase 2: 迁移 application 核心模块

- 迁移：
  - `session_plan`
  - `session_context`
  - `bootstrap_plan`
  - `task/config`
  - `address_space/*`
  - `context/*`
  - `project/context_builder`
  - `story/context_builder`

### Phase 3: 建立边界转换层

- 在 API / executor 侧补充：
  - application runtime ↔ executor config
  - application address space ↔ executor address space
  - application MCP server ↔ ACP MCP server
  - application file entry ↔ relay file entry

### Phase 4: 收口前端状态

- 移除 `storyStore.stories`
- 页面改为从 `storiesByProjectId` 或按 id 查询函数读取
- 保证 SessionPage / StoryTabView 行为不退化

### Phase 5: 测试与回归

- application 单元测试
- API 编译/类型检查
- 前端类型检查
- 关键后端 crate `cargo check`

## Verification Matrix

### 编译与类型

- [x] `cargo check -p agentdash-application`
- [x] `cargo check -p agentdash-api`
- [x] `cargo check -p agentdash-local`
- [x] `pnpm --filter frontend exec tsc --noEmit`

### 定向测试

- [x] session plan / bootstrap plan 相关测试通过
- [x] address space 相关测试通过
- [x] workflow store / story store 相关前端测试通过（当前仓库无现成前端单测，已用类型检查与调用面回归替代）

### 行为回归

- [x] task session context snapshot 仍能正确返回
- [x] story session / project session context snapshot 仍能正确返回
- [x] workspace files 行为保持“必须显式 root_path”约束
- [x] story 看板 / session 页仍能正确读取 story 数据

## Checklist

- [x] 建立任务目录
- [x] 写入 PRD / checklist / 验证矩阵
- [x] 初始化 task context
- [x] 激活当前任务
- [x] 建立 application runtime 自有类型
- [x] 迁移 task/config 到 application runtime 类型
- [x] 迁移 session_context / session_plan / bootstrap_plan
- [x] 迁移 address_space 模块到 application runtime 类型
- [x] 迁移 context / project / story builder
- [x] 建立 API / executor 边界转换函数
- [x] 收口 storyStore 单一事实源
- [x] 跑后端编译检查
- [x] 跑前端类型检查
- [x] 更新 checklist 为完成态

## Implementation Notes

### Backend

- 新增 `crates/agentdash-application/src/runtime.rs`，把 executor config、thinking level、mount/address space、MCP server、file entry 全部收口到 application 自有 runtime 值模型
- `agentdash-application` 核心模块已改为消费自有 runtime 类型：`task/config`、`session_context`、`session_plan`、`bootstrap_plan`、`address_space/*`、`context/*`、`project/context_builder`、`story/context_builder`
- 新增 `crates/agentdash-api/src/runtime_bridge.rs`，集中承担 runtime ↔ executor / ACP / relay 的显式转换
- `agentdash-api` 路由、gateway、address space access 已改为“内部走 application runtime，边界才做转换”，避免 application 模型继续被外部 DTO 侵入

### Frontend

- `storyStore` 已去除扁平 `stories` 冗余事实源，只保留 `storiesByProjectId`
- 新增 `findStoryById` / `flattenStoriesMap` 辅助函数，用于从单一事实源导出读取视图
- `StoryPage`、`SessionPage` 已改为通过 `storiesByProjectId` + 查找函数读取 story，不再依赖重复缓存

## Verification Evidence

- `cargo check -p agentdash-application`
- `cargo check -p agentdash-api`
- `cargo check -p agentdash-local`
- `cargo test -p agentdash-application`
- `cargo test -p agentdash-application --no-run`
- `cargo test -p agentdash-api --lib`
- `cargo test -p agentdash-api --no-run`
- `pnpm --filter frontend exec tsc --noEmit`

## Progress Log

- 2026-03-26：建立 task 目录、PRD、验证矩阵与 checklist
- 2026-03-26：完成 application runtime 自有模型与 application 层签名迁移
- 2026-03-26：完成 API runtime bridge，恢复 `agentdash-api` 编译与测试
- 2026-03-26：完成 `storyStore` 单一事实源收口，并修复 `StoryPage` / `SessionPage` 消费路径
- 2026-03-26：完成后端/前端验证并将 checklist 回写为完成态

## Notes

- 本次重构允许较大范围改签名，但必须保持对外 REST 行为和前端消费形状稳定
- 若某一步需要进一步拆分，将在同一任务目录继续追加计划，不缩减目标

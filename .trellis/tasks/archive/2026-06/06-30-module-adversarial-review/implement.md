# 执行计划

## 1. Module Topology Pass

- [x] 汇总 Rust workspace crate、pnpm package、API route、frontend feature 的入口清单。
- [x] 读取旧任务 `06-14-module-overdesign-review` 的报告和 research，提取 baseline 问题索引。
- [x] 对 8 个候选 review domain 做证据扫描：
  - [x] Orchestrated Work Surface。
  - [x] Agent Runtime Session Surface。
  - [x] Extension / Workspace Module Runtime Surface。
  - [x] Authority & Capability Runtime。
  - [x] VFS & Runtime Tool Surface。
  - [x] Local Runtime & Relay Surface。
  - [x] Project / Workspace / Backend Placement。
  - [x] Knowledge & Context Surface。
- [x] 输出 `module-topology.md`，每个 domain 记录事实源、入口、依赖、前端消费面、baseline 对照和是否适合独立 subagent。
- [x] 用户确认以 8 个候选 domain 为起点，并允许 review 过程松弛挑战/合并/拆分边界。

## 2. Configure Subagent Context

- [x] 根据确认后的分工创建或更新 `implement.jsonl`。
- [x] 根据审查标准创建或更新 `check.jsonl`。
- [x] 为每个 subagent 准备相同的审查准则和不同的模块边界。

## 3. Adversarial Review Pass

- [x] 派发 subagent 并行审查确认后的模块。
- [x] 收集 research 产物到本任务目录。
- [x] 主会话二次复核证据路径和问题分类。
- [x] 去重、合并跨模块问题，识别真正 owner。

## 4. Synthesis

- [x] 输出综合报告 `adversarial-review.md`。
- [x] 对每个问题给出优先级、影响面和建议收束边界。
- [x] 标记适合拆后续实现任务的问题。
- [x] 对照旧任务 baseline 标记 resolved / residual / resurfaced / superseded。
- [x] 输出 `cleanup-scope-triage.md`，按 Quick / Medium / Design 统计后续清理范围。
- [x] 新建 `followups/`，记录 Design 后续讨论清单与快速收束任务映射。
- [x] 创建 `architecture-quick-convergence` 父任务及 5 个独立工作项子任务。

## Validation

- `git status --short`
- 人工复核报告中所有证据路径真实存在。
- 不运行全量测试；本任务默认不改业务代码。

## Stop Gates

- module topology 未经用户确认前，不派发正式 subagent。
- 如果 topology pass 证明候选 domain 需要合并或拆分，先更新 PRD / design / implement，再继续。
- 如审查发现必须立即修复的问题，先记录为候选实现任务，不在本任务中直接修改业务代码。

# AgentDash Reviews

此目录存放阶段性架构、体验和重构 review。review 文档用于保留观察、问题分级和路线建议；稳定契约应沉淀到 `.trellis/spec/`。

## 批次索引

| 批次 | 主题 | 建议阅读顺序 |
| --- | --- | --- |
| [2026-05-16-zip-static-review](./2026-05-16-zip-static-review/) | 基于早期 zip 快照的全局静态 review 与 session 定向 review | 先读 `architecture-module-review.md`，再读 `session-launch-refactor-plan.md` |
| [2026-05-19-design-language-audit](./2026-05-19-design-language-audit/) | 前端设计语言审计 | 读 `frontend-design-language-audit.md` |
| [2026-05-23-architecture-review-round](./2026-05-23-architecture-review-round/) | 两份外部架构 review 源文档及汇总路线图 | 先读 `architecture-review-synthesis.md`，需要原始依据时再读两份源 review |
| [2026-05-26-compaction-session-branching-review](./2026-05-26-compaction-session-branching-review/) | compaction、model context projection、session branching / lineage 对照 review | 先读 `03-findings.md`，需要代码证据时读 `00-context-index.md` 与 `01-current-implementation.md` |

## 当前重点批次

[2026-05-23-architecture-review-round](./2026-05-23-architecture-review-round/) 是当前后续架构重构的主要入口：

| 文档 | 说明 |
| --- | --- |
| [architecture-review-synthesis.md](./2026-05-23-architecture-review-round/architecture-review-synthesis.md) | 两份架构 review 的异同、当前代码校准、后续重构优先级 |
| [database-schema-source-decision.md](./2026-05-23-architecture-review-round/database-schema-source-decision.md) | PostgreSQL migrations 与 SQLite 本机缓存初始化的 schema 事实源决策 |
| [frontend-backend-contract-generation-strategy.md](./2026-05-23-architecture-review-round/frontend-backend-contract-generation-strategy.md) | 前后端协议生成标准化方案、候选方案取舍与 DTO 迁移顺序 |
| [module-boundary-split-plan.md](./2026-05-23-architecture-review-round/module-boundary-split-plan.md) | Workflow、VFS、Relay protocol、Agent loop 的分批拆分顺序 |
| [runtime-control-plane-review.md](./2026-05-23-architecture-review-round/runtime-control-plane-review.md) | 偏 Runtime 控制平面、Session pipeline、Relay 时序和 crate 分层 |
| [platform-boundary-governance-review.md](./2026-05-23-architecture-review-round/platform-boundary-governance-review.md) | 偏平台边界、工程治理、AppState、schema、Plugin API 和前端契约 |

[2026-05-26-compaction-session-branching-review](./2026-05-26-compaction-session-branching-review/) 是 compaction 与 session branching 后续收敛的入口：

| 文档 | 说明 |
| --- | --- |
| [00-context-index.md](./2026-05-26-compaction-session-branching-review/00-context-index.md) | 代码索引、subagent 分工、后续核查入口 |
| [01-current-implementation.md](./2026-05-26-compaction-session-branching-review/01-current-implementation.md) | 当前项目 compaction / projection / lineage 数据流 |
| [02-reference-comparison.md](./2026-05-26-compaction-session-branching-review/02-reference-comparison.md) | Codex、Claude Code、pi-mono 结构对照 |
| [03-findings.md](./2026-05-26-compaction-session-branching-review/03-findings.md) | 风险分级、架构判断与分阶段建议 |

## 命名规则

- 批次目录使用 `YYYY-MM-DD-topic`。
- 文档文件名描述 review 主题，不使用 `review1.md` / `review2.md` 这类临时名。
- 源 review 与汇总文档放在同一批次目录；跨批次引用使用相对链接。

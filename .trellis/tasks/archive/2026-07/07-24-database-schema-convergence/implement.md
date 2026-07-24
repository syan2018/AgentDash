# 数据库表收束与迁移基线压缩实施计划

## 1. 锁定现有行为

- [x] 为 Gate waiter/parent continuation 的 register、claim、lease retry、completion、replay 补齐定向测试。
- [x] 为 Canvas observation/snapshot 独立 roundtrip 与互不覆盖补齐测试。
- [x] 记录最终 46 张业务表清单及保留理由。

## 2. 删除明确冗余结构

- [x] 删除 `views` 表对应的 domain type、Repository 方法、PostgreSQL SQL、装配和前端遗留 state/type。
- [x] 删除未使用的 `agent_run_control_effects` readiness 与 schema。
- [x] 删除 Terminal outbox/correlation 写入、domain 冗余 contract 与 schema。
- [x] 删除未使用的 `AgentRunLineageRepository` trait、实现和 `RepositorySet` 装配，保留 fork graph store。
- [x] 对已删除符号与表名执行生产路径负向搜索。

## 3. 收回 Gate owner state

- [x] 定义 typed `GateResultDeliveryState` 并挂到 `LifecycleGate` owner。
- [x] 将 marker Repository mutation 改为锁定并更新 `lifecycle_gates.delivery`。
- [x] 删除 `gate_result_delivery_markers` table contract。
- [x] 运行 Companion Gate 定向测试。

## 4. 合并 Canvas state

- [x] 定义单一 `agent_run_canvas_state` mapping。
- [x] 将 observation/snapshot upsert 改为 column-specific mutation。
- [x] 更新读取、readiness、API 和 runtime tool 调用。
- [x] 删除两张旧 latest-state table contract。
- [x] 运行 Canvas Repository/Application/API 定向测试。

## 5. 重建首发 baseline

- [x] 将最终 schema 汇总为新的 `migrations/0001_init.sql`。
- [x] 删除 `0002`～`0116` migration 文件。
- [x] 核对 46 张业务表、主键、外键、CHECK、唯一约束和索引。
- [x] 使用全新 PostgreSQL 数据库手动执行 baseline、写入 SQLx metadata 并验证 readiness。
- [x] 重建项目 embedded PostgreSQL database。
- [x] 运行 `ALLOW_MIGRATION_BASELINE_REWRITE=1 pnpm run migration:guard`。

## 6. 文档与质量门

- [x] 更新数据库规范中的最终表边界和 baseline 版本事实。
- [x] 更新仍提及已删除表或 116 migration 的现行文档。
- [x] 定向格式化受影响 Rust/TS 文件。
- [x] 运行相关 Rust tests/check、前端 typecheck 或定向 tests。
- [x] 最后运行 migration guard、空库 migration/readiness 与全量负向搜索。

## 验证命令

```powershell
$env:ALLOW_MIGRATION_BASELINE_REWRITE='1'
pnpm run migration:guard
Remove-Item Env:ALLOW_MIGRATION_BASELINE_REWRITE
```

```powershell
cargo test -p agentdash-infrastructure gate_result
cargo test -p agentdash-infrastructure canvas_runtime
cargo test -p agentdash-infrastructure agent_run_fork
cargo check -p agentdash-domain -p agentdash-application -p agentdash-infrastructure -p agentdash-api
```

最终按实际受影响 package 补充前端 typecheck/test。若 workspace Cargo lock 被 rust-analyzer 占用，
先观察进程并等待；若 `cargo fmt --all` 受 reference checkout 或文件映射影响，使用同 toolchain
对任务文件定向 `rustfmt --edition 2024`。

## 回滚点

- Gate 与 Canvas 每个阶段分别保持可编译、可测试提交边界。
- baseline 替换只在最终 schema 与代码均稳定后进行。
- baseline 前的 Git 历史是唯一回滚来源；不为旧数据库保留运行时兼容分支。

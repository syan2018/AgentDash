# 执行计划：统一后端可观测层

## 顺序清单

### 阶段 0：脚手架（不动调用点，先把基础设施立起来）
1. 新建 crate `agentdash-diagnostics`（加入 `Cargo.toml` workspace members + `workspace.dependencies`）。
   - 依赖：`tracing`、`tracing-subscriber`、`serde`、`serde_json`。
2. workspace 依赖补 `tracing-appender = "0.2"`（`Cargo.toml [workspace.dependencies]`），`agentdash-api` 引用。
3. 在 `agentdash-diagnostics` 实现：
   - `Subsystem` 取值集合（枚举或 `&'static str` 常量）。
   - `diag!` 宏（展开为 `tracing::event!`，强制 subsystem，透传字段与 message）。
   - `DiagnosticRecord` + visitor（抽取 subsystem/session_id/run_id/backend_id 等已知字段）。
   - `DiagnosticLayer`（impl `Layer`）+ `DiagnosticBuffer`（`Arc<RwLock<VecDeque<_>>>`，有界，`query(filter)`）。
4. 单测：`diag!` 产出带 subsystem 的 event；`DiagnosticLayer` 入缓冲与过滤 query。

**Review gate**：facade API 形状确认后再铺迁移。

### 阶段 1：订阅器装配 + 文件落地（仅 agentdash-api）
5. `agentdash-api/src/main.rs`：fmt().init() → Registry 三层（env + fmt + file + diagnostic），持有 `WorkerGuard`。
6. `build_json_file_layer()`：`tracing_appender::rolling::daily`，目录取 `AGENTDASH_LOG_DIR` 默认 `./logs/`。
7. `DiagnosticBuffer` 透传：main → `run_server` → `bootstrap` → `AppState`（新增字段）。
8. `.gitignore` 加 `/logs/`（确认未被提交）。

**验证**：本地起 api，确认 stdout 仍 pretty、`./logs/agentdash-api.log.<date>` 出现 JSON line。

### 阶段 2：查询端点
9. 新建 `agentdash-api/src/routes/diagnostics.rs`：`GET /api/diagnostics`，参数 subsystem/session_id/run_id/backend_id/level/since_ms/limit。
10. `routes.rs` 的 `secured_api` merge `diagnostics::router()`；`routes.rs` 顶部 `pub mod diagnostics;`。
11. 端点从 `AppState.diagnostics` 读快照过滤返回；limit 设上限防滥用。

**验证**：`curl` 带鉴权打 `/api/diagnostics?subsystem=relay&limit=20` 返回近期诊断。

### 阶段 3：全量迁移调用点（机械 + 人工补字段）
12. 脚本辅助机械迁移：把全 workspace `tracing::{info,warn,error,debug,trace}!(...)` → `diag!(<Level>, <Subsystem>, ...)`。
    - subsystem 初值按 crate/模块映射一张表批量赋；脚本无法判定的留 TODO 标记。
    - 注意保留原字段与 message 顺序；`tracing::` 前缀与直接 `info!`（已 use 引入）两种写法都要覆盖。
13. 人工过一遍**热点诊断路径**补关联字段（session_id/run_id/backend_id），重点：`relay/ws_handler.rs`、`session/launch/*`、`agent_run/*`、`reconcile/*`。
14. 移除各文件不再需要的 `use tracing::…;`，按需 `use agentdash_diagnostics::diag;`。

### 阶段 4：定向补埋点（哑子系统）
15. 给以下 crate 关键路径补 `diag!`（实测几乎无日志）：
    - `agentdash-application-lifecycle`（dispatch / 状态转换关键节点）
    - `agentdash-application-workflow`（脚本编译 / orchestration 执行）
    - `agentdash-application-hooks`（hook 触发 / 失败）
    - `agentdash-application-skill`（discovery / 装配）
    - 颗粒度：进入/失败/关键决策点，info/warn/error 即可，不追求全覆盖。

### 阶段 5：防回退 + 收尾
16. workspace 根新建 `clippy.toml`，`disallowed-macros` 禁 5 个裸 tracing 宏。
17. 跑 `cargo clippy --workspace --all-targets -- -D warnings`，清掉残留裸宏（暴露漏迁的调用点，回到阶段 3 补）。
18. 确认/补 CI 跑 clippy 的步骤。
19. spec 更新：写一份 subsystem 取值约定 + "诊断只走 diag!" 的 spec（`.trellis/spec/backend/`）。

## 验证命令
```bash
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings   # 含 disallowed-macros 守门
cargo test --workspace                                   # 现有测试不回归
# 手动：起 agentdash-api，验 ./logs/ JSON 文件 + GET /api/diagnostics
```

## 风险文件 / 回滚点
- `agentdash-api/src/main.rs`：订阅器装配是核心改动；`WorkerGuard` 丢失会静默丢文件日志——重点检查。
- `app_state.rs` / `bootstrap/*`：透传 `DiagnosticBuffer` 改了构造签名，编译期可暴露遗漏。
- 迁移波及 ~100+ 文件：分 crate 提交，便于 `git revert` 局部回滚。
- 回滚顺序（见 design.md）：端点 → 文件层 → DiagnosticLayer → 最坏保留 diag!（等价原 tracing）。

## 提交策略
按 memory 约定直接提交 main，只 stage 相关文件，默认不 push。建议分 commit：
1. `agentdash-diagnostics` crate + facade（阶段 0）
2. 订阅器 + 文件 + 端点（阶段 1-2）
3. 全量迁移（阶段 3，可按 crate 再拆）
4. 补埋点 + clippy 守门 + spec（阶段 4-5）

## 完成前检查
- [ ] PRD 全部 AC 勾选。
- [ ] clippy 守门生效（故意写一处裸 `tracing::info!` 应 CI 失败）。
- [ ] 领域通道（context audit / session events / lifecycle / shell）未被牵动。

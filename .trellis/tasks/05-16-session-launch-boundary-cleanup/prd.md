# Session 重构边角清理与 Launch 边界收敛

## Goal

完成 session refactor 后剩余边角清理，优先消除已发现的实际风险：

- owner session 的 Plain 续跑仍可能跳过 owner construction，导致冷启动 follow-up 走 cached/default VFS/MCP/capability。
- `claim_prompt` 后的早期错误路径未释放 `Claimed`，可能把 session 卡死在“运行中”。
- Task/local relay 等来源仍存在 MCP/Construction 语义缝隙，需要确认并收口。

同时评估并落地一版低风险的 Construction / Launch 边界收敛：Construction 负责“事实解析”，Launch 只负责“单轮执行策略与副作用执行”。

## Requirements

- 保持当前预研项目最正确状态，不引入兼容性路径或回退方案。
- 生产 prompt 入口继续只走 `LaunchCommand -> SessionLaunchService`。
- owner session 在 Plain、OwnerBootstrap、RepositoryRehydrate 三种生命周期下都应复用同一套 owner construction 事实解析。
- `TurnSupervisor::claim_prompt` 后，任何规划前错误、meta 读取错误、runtime command store 错误、planner 错误都必须释放 turn claim。
- runtime command store 仍是 pending capability transition 的事实源，connector accepted 后才标记 applied。
- Construction / Launch 边界调整必须以小步迁移为主：本任务落地高价值、低风险部分，并记录后续彻底拆分方案。
- 不派发 subagent。

## Acceptance Criteria

- [ ] `cargo test -p agentdash-application session::launch` 通过。
- [ ] `cargo test -p agentdash-application session::construction` 通过。
- [ ] `cargo test -p agentdash-application session::turn_supervisor` 通过。
- [ ] 覆盖 `claim_prompt` 后早期错误释放 claim 的测试。
- [ ] 覆盖 owner Plain 续跑仍携带 owner VFS/MCP/capability construction 的测试或等价断言。
- [ ] `rg "PreparedSessionInputs|finalize_request|finalize_augmented_request|SessionLaunchIntent|PromptSessionRequest" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src` 生产代码无命中。
- [ ] 输出 Construction / Launch 边界最终收敛评估，说明哪些已落地、哪些应在后续任务迁移。

## Notes

- 相关 spec：`.trellis/spec/backend/session/session-startup-pipeline.md`、`runtime-execution-state.md`、`execution-context-frames.md`、`bundle-main-datasource.md`。
- 关键文件：`crates/agentdash-api/src/bootstrap/session_construction_bootstrap.rs`、`crates/agentdash-application/src/session/prompt_pipeline.rs`、`launch_planner.rs`、`construction_planner.rs`。

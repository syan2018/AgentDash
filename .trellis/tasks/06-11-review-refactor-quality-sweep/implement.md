# Implement: 执行计划

## 0. 初始化

- [x] 创建专用分支 `codex/review-refactor-quality-sweep`。
- [x] 创建 Trellis 主任务。
- [x] 写入 `review-index.md`、`architecture-backlog.md`、`reviews/`、`fixes/`、`tool-runs/`。
- [x] 启动 Trellis 主任务。

## 1. 首轮工具扫描

- [ ] 按模块运行 `fuck-u-code`。
- [ ] 记录每个模块的分数、热点文件、timeout、parser 限制。
- [ ] 选出第一批并行 review 模块。

## 2. 并行 review 循环

- [ ] 给每个 review subagent 明确模块边界和输出格式。
- [ ] 主控汇总 review 结果。
- [ ] 将架构项写入 `architecture-backlog.md`。
- [ ] 将实现级模块修复项写入 `review-index.md` 待处理队列。

## 3. 模块级修复循环

- [ ] 选择互不冲突的模块修复并行派发。
- [ ] 每个修复完成后主控 review diff。
- [ ] 运行最小必要 check。
- [ ] 更新 `fixes/` 记录。
- [ ] 以模块为单位提交。

## 4. 持续收敛

- [ ] 回到工具扫描和并行 review。
- [ ] 定期整理 `architecture-backlog.md` 的优先级。
- [ ] 当主要模块表面质量问题清理完成后，执行完整质量检查并准备总结。

## 验证命令基线

- 前端模块：`pnpm --filter app-web run typecheck`，必要时加 `pnpm --filter app-web run lint` 或定向 test。
- Rust/backend 模块：按触及 crate 选择 `cargo check` / `cargo test` 的最小范围。
- 全局收尾：按风险决定是否运行 `pnpm --filter app-web run check`、`cargo test` 或项目级验证。

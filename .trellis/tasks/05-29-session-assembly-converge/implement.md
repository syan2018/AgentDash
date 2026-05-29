# session 装配流水线收敛执行计划

## 预检

- [x] 读取 PRD、backend/session/capability/VFS specs。
- [x] 派发 resolver 与 VFS 单源派生两个 explorer 复核任务。
- [ ] 记录本轮 resolver 复核结论到 PRD / journal。
- [x] 启动 Trellis child：`python ./.trellis/scripts/task.py start .trellis/tasks/05-29-session-assembly-converge`。

## 实施顺序

1. `SessionAssemblyBuilder` 拆分
   - 新建 `crates/agentdash-application/src/session/assembly_builder.rs`。
   - 从 `assembler.rs` 迁出 builder struct、builder impl 与 `apply_session_assembly`。
   - 在 `session/mod.rs` 或 assembler 内部以最小 public 面重新引用。
   - 运行 `cargo check -p agentdash-application`。
   - 状态：已完成；`assembler.rs` 2690 -> 2326 行；`cargo check -p agentdash-application` 通过。

2. `compose_owner_bootstrap` / `compose_story_step` 拆小
   - 先抽 owner bootstrap 的 VFS 准备 helper，保持输入输出显式。
   - 再抽 story step executor/VFS/context binding helper。
   - compose 入口只保留顺序编排、错误映射和最终 builder 输出。
   - 用行数检查确认两个 compose 函数均 < 80 行。
   - 状态：已完成；`compose_owner_bootstrap` 约 66 行，`compose_story_step` 约 51 行。

3. VFS 投影集中同步
   - 在 `SessionConstructionPlan` 上新增 active VFS 访问器。
   - 调整 `apply_session_assembly` 与 `finalize_session_construction_projection`，通过 `set_active_vfs` / `sync_vfs_projection_from_capability` 集中同步 `surface.vfs` / `context_projection.vfs`。
   - 保留 launch 前一致性校验。
   - grep 验证成对赋值消除。
   - 状态：已完成；`SessionConstructionPlan` 提供 active VFS accessor/sync helper，apply/finalize 不再手工成对赋值。

4. 文档与验收
   - 更新 PRD wave2 核验结论、完成证据与建议人工 review 标记。
   - 更新 progress checklist 和 developer journal。
   - 运行 `cargo check --workspace`。
   - 运行 `cargo test -p agentdash-application --lib`。
   - 提交实现与归档提交。
   - 状态：`cargo check --workspace` 通过；`cargo test -p agentdash-application --lib` 595 passed；`session::assembler` 22 passed。

## 风险点

- `assembler.rs` 内部 helper 使用大量私有 spec 类型，拆文件时先只迁 builder，避免扩大可见性面。
- Query 路径只读 snapshot 不等同 launch bundle，resolver 复核未完成前不抽跨路径统一返回类型。
- VFS 同步 helper 要保持 runtime command replay 后的 effective VFS 为权威结果。

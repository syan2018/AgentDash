# 进一步拆分后端大模块

## Goal

评估后端 application 侧剩余大模块的 crate 边界，形成可执行的后续拆分计划。计划覆盖 `workflow`、`hooks`、`shared_library`。

## Requirements

- 明确 `workflow` 从 `agentdash-application-lifecycle` 中摘出为独立 crate 后的完整文件归属，以及它与 lifecycle、AgentRun、VFS、runtime session 的依赖方向。
- 明确 `hooks` 模块的独立 crate 边界，包括 rule/preset/script engine/provider 与 active workflow projection 的归属。
- 明确 `shared_library` 模块的独立 crate 边界，包括 marketplace、seed、install、publish 与 `skill_asset` / extension package 的关系。
- 评估当前 crates 依赖关系是否支持这些拆分，列出必须建立的端口、DTO、RepositorySet、禁止重新出现的旧引用关系和测试迁移问题。
- 输出可分阶段执行的拆分顺序；每阶段必须明确完整搬运范围、旧路径删除要求、禁止引用关系、验证命令和停止条件。
- 每阶段执行顺序必须遵循：先把文件移动到目标状态并删除旧模块入口，再修复由移动造成的编译错误。
- 编译修复只允许处理模块路径、imports、Cargo 依赖、类型归属、trait wiring、constructor wiring 和测试路径；禁止为了让项目通过编译而修改模块业务行为。
- 禁止为了切断或绕开向 `agentdash-application` 的依赖链，在新 crate 内重复实现已有接口、DTO、repository trait、port、service contract、错误类型或 helper；必须复用原接口、迁移原接口，或把接口上移到 `application-ports` / `domain` / `spi`。
- 本任务只做 review 与 planning，不直接修改业务代码或启动实现。

## Acceptance Criteria

- [x] 已完成并行模块边界 review，结论包含 workflow、hooks、shared_library。
- [ ] `design.md` 记录建议 crate 边界、依赖方向、关键迁移策略和被拒绝方案。
- [ ] `implement.md` 记录可执行拆分步骤、每步验证命令、风险文件和依赖顺序。
- [ ] 计划符合当前项目约束：不保留兼容性方案，不回滚并行工作区修改，不把 infrastructure 反向绑定到 application；每阶段都以完整搬运后的目标状态为验收对象。
- [ ] 每阶段都包含行为不变检查：不得修改默认值、过滤条件、执行顺序、错误映射、权限判断、持久化语义、事件语义或 runtime projection 语义来换取编译通过。
- [ ] 每阶段 check review gate 都明确评估重复实现风险：不得出现为了规避 application 依赖而复制既有接口或业务 helper 的实现。

## Notes

- 用户决策：`workflow` 从 lifecycle 中摘出；workflow 与 lifecycle 更多是归属关系，拆分计划必须直接面向目标状态。
- 用户倾向：`hooks` 与 `shared_library` 也应拆分。

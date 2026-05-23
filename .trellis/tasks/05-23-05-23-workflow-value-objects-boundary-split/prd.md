# Workflow value objects 目录级拆分

## Goal

将 workflow/value_objects.rs 按 contract、lifecycle、activity、run_state、tool capability、validation 等语义拆分，保持 re-export 与序列化行为稳定。

## Requirements

- 将 `crates/agentdash-domain/src/workflow/value_objects.rs` 按语义拆成多个文件，优先顺序为 validation、tool capability、activity/lifecycle/run state、contract。
- 保持 `workflow::mod.rs` public re-export 不变，调用方不需要改 import。
- 不改变 serde tag、enum variant、数据库 JSON payload 形态。
- 每批移动后运行 `cargo check -p agentdash-domain -p agentdash-application` 和 workflow/domain 相关测试。

## Acceptance Criteria

- [ ] `value_objects.rs` 不再承载 validation、tool capability、activity/lifecycle/run state 的全部定义。
- [ ] 新模块命名与 workflow architecture 语义一致。
- [ ] public re-export 清晰，旧调用方编译通过。
- [ ] 相关 check/test 通过。

## Notes

- Keep `prd.md` focused on requirements, constraints, and acceptance criteria.
- Lightweight tasks can remain PRD-only.
- For complex tasks, add `design.md` for technical design and `implement.md` for execution planning before `task.py start`.

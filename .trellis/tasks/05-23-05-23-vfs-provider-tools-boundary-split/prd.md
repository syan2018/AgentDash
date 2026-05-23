# VFS provider/tools/mutation 边界拆分

## Goal

将 VFS 按 core、providers、tools、mutation、materialization、surface 边界整理，保持 mount 语义和 agent tool 行为稳定。

## Requirements

- 将 VFS 先按 `core`、`providers`、`tools`、`mutation`、`materialization`、`surface` 建立可见目录边界。
- 优先拆 agent-callable FS tools 与 provider 实现，保留现有 facade/re-export。
- VFS mount、capability、materialization、mutation queue 语义保持稳定。
- 每批移动后运行 `cargo check -p agentdash-application -p agentdash-api -p agentdash-local` 以及 VFS 相关测试。

## Acceptance Criteria

- [ ] 至少一个 VFS 子域完成文件级拆分。
- [ ] 调用方 public import 不扩散到具体实现文件。
- [ ] VFS tool/provider 行为测试或 cargo check 通过。
- [ ] spec 记录 VFS 子域边界原因。

## Notes

- Keep `prd.md` focused on requirements, constraints, and acceptance criteria.
- Lightweight tasks can remain PRD-only.
- For complex tasks, add `design.md` for technical design and `implement.md` for execution planning before `task.py start`.

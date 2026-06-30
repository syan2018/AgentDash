# AgentRun admission production boundary 收束

## Goal

实现 design backlog Slice 4 / D1：让 AgentRun effective capability/admission 成为生产工具执行边界；wire admit_tool 到真实 tool invocation entry；denied decision 必须阻断 tool.execute；旧 RuntimeSession capability-state-only admission 语义收窄为 schema exposure / visible state，不再被误当授权边界。

## Requirements

- AgentRun 必须成为工具执行准入的生产事实 owner：`AgentRunEffectiveCapabilityPort::admit_tool` 需要有产品路径实现，并被真实 tool invocation entry 消费。
- 工具 schema exposure 与工具 execution admission 必须分离：
  - visible `CapabilityState` 继续表达模型可见工具、VFS、MCP、workspace module surface；
  - tool-level `PermissionGrant` 只进入 frame-scoped admission projection；
  - execution admission 只能在执行入口判定，不得通过 mutating `CapabilityState` 或 provider-local permission check 表达。
- Denied admission 必须阻断 `tool.execute`。阻断结果要通过现有 tool result/error decision 语义返回，不引入 provider panic 或另一个平行审批系统。
- `RuntimeSessionEffectiveCapabilityPort::execution_capability_state_for_runtime_session` 这条旧语义不得继续被误读为授权边界。若当前切片不能完全删除端口，必须把它收窄为 schema-facing state/view projection，并在实现与 spec 中明确它不是 Grant admission。
- 运行时 tool-policy bridge 可以先通过现有 `AgentRuntimeDelegate::before_tool_call` 接入；本切片不展开 D6 delegate-set 大拆分，但实现必须为后续迁移到 `RuntimeToolPolicyDelegate` 保持清晰边界。
- 清理旧问题优先于添加 feature：不得通过新增一条平行 permission/adapter 路径来“解决”问题，必须删除、收窄或重新命名旧的概念分叉。
- 项目处于预研未上线阶段，不做兼容层、回退路径或旧字段保留，除非代码边界暂时无法一次删除且文档中记录为后续 D6/D9 残留。
- Subagent 执行约束：实现 worker 只做 scoped 搜索、编辑、format 或小型定向测试；不要自行跑大规模 Rust 编译或 broad suites，昂贵编译放到 check/integration 阶段。

## Acceptance Criteria

- [x] 存在产品路径 `AgentRunEffectiveCapabilityPort` 实现，能基于 runtime session / AgentRun frame anchor 产出 effective view 与 admission decision。
- [x] 真实工具执行路径在 `tool.execute` 前调用 AgentRun admission；无 active visible tool / grant 时 deny，deny 后不调用工具实现。
- [x] tool-level grant 只按当前/effect frame 生效；frame A 的 grant 不允许 frame B 的工具执行。
- [x] visible `CapabilityState` 不因 tool-level grant 扩大 `tool.capabilities`、`enabled_clusters` 或 `tool_policy.include_only`。
- [x] 旧 runtime-session capability-state-only 端口不再承担或暗示 execution authorization；调用点和命名/注释/spec 均表达 schema exposure 或 visible-state projection。
- [x] 相关单元测试覆盖 allow、deny、frame-scoped grant、deny prevents execute。
- [x] 静态检查能证明生产代码存在 `admit_tool` 调用，且未重新引入 grant projection 写回 `CapabilityState` 的路径。
- [x] Trellis spec 更新记录可复用边界：AgentRun admission 是 Grant 执行准入 owner，provider `CapabilityState` guard 只做 declarative exposure/local invariant。

## Notes

- Source: `.trellis/tasks/06-30-design-backlog-review/design-review.md#d1-agentrun-visible-capability--admission`.
- This is Slice 4 from `.trellis/tasks/06-30-design-backlog-review/implementation-slices.md`.

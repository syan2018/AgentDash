# Runtime context discovery 单入口收束

## Goal

将 `FrameLaunchEnvelope` 收束为清晰的 launch projection 边界，并把 AGENTS.md、Skill、Memory 等 runtime context discovery 收束为 launch-time 单入口，消除 frame construction / AgentRun / frontend 展示链路中散落的发现与投递路径。

## Background

Runtime context discovery 的输入事实来自最终 launch surface，尤其是当前可见 VFS、当前身份、skill/memory provider 列表和 connector delivery profile。AGENTS.md、Skill、Memory 都属于“基于当前 runtime surface 派生的模型可见上下文”，它们应该在同一条 launch-time projection 中产生，并由同一个结果对象进入 `FrameLaunchIntent` / `LaunchPlan` / `ContextFrame`。

当前发现能力已经有部分底层共用，但不同调用路径仍可能在 frame construction、AgentRun capability projection、owner bootstrap、existing surface launch 和前端展示之间分叉。该任务要把“发现文件”和“把发现结果投递到模型上下文”都收束为显式契约，避免某条启动路径只更新了 capability 或 memory，却漏掉 system guidelines。

`FrameLaunchEnvelope` 当前平铺承载 command intent、runtime surface、context projection、diagnostics 和 frame refs，导致调用方难以判断事实源与派生物。runtime context discovery 的单入口需要建立在更清晰的 envelope 分层上，否则新的 discovery output 仍会继续作为零散字段被 route 层手动传递。

## Scope

- 后端 runtime context discovery orchestration。
- `FrameLaunchEnvelope` 内部封装与 launch projection 分层。
- VFS mount file discovery 底层扫描能力。
- AGENTS.md / project guidelines、Skill、Memory 的 discovery adapter。
- launch context projection（envelope `context` 分组）、`LaunchPlan`、`TurnPreparer` 中 discovery output 的传递契约。
- ContextFrame / delivery plan 中 `identity`、`system_guidelines`、`memory_context`、capability delta 的可观察结果。

## Non-goals

- 不重新设计 ProjectAgent prompt 文案或 Persona 内容。
- 不调整用户偏好、AGENTS.md 文件格式或 Skill/MEMORY.md 文件格式。
- 不引入兼容性回退路径；现阶段以正确的 runtime projection 形态为准。

## Requirements

- 提供一个 launch-time 单入口，例如 `RuntimeContextDiscovery` / `LaunchContextDiscovery`，以最终 `FrameLaunchSurface.vfs`、`AuthIdentity`、skill discovery providers、memory discovery providers 和必要 runtime options 为输入。
- `FrameLaunchEnvelope` 顶层字段应按语义分组，至少区分 command intent、runtime surface、context projection、diagnostics/frame refs，避免新增 discovery 数据继续平铺在顶层。
- command intent 只表达用户请求输入、环境变量、身份与 terminal hook binding；runtime surface 只表达闭包后的 VFS/capability/MCP/execution profile、working directory 与 backend anchor；context projection 只表达 bundle、guidelines、memory 等 prompt-context 输入。
- 单入口输出结构化 discovery result，至少包含：
  - `discovered_guidelines: Vec<DiscoveredGuideline>`
  - `discovered_memory: MemoryDiscoveryOutput`
  - skill baseline / skill diagnostics 所需的 session capability projection
- `FrameLaunchEnvelope` 构建阶段必须从该单入口获得 discovery result；ProjectAgent、LifecycleNode、ExistingSurface、companion modifier 等 launch route 不应各自决定是否派生 guidelines/memory。
- discovered guidelines/memory 归入 envelope `context` projection（`FrameLaunchContextProjection`），不再作为 `FrameLaunchIntent` 字段；`FrameLaunchIntent` 只表达命令请求事实（input/env/identity/terminal hook binding）。
- route 层不应手写空的 discovered guidelines/memory；`system_guidelines` frame 是否出现只由 discovery result 和 frame builder 的空内容过滤决定。
- AGENTS.md、Skill、Memory 的 VFS 文件扫描使用 `agentdash-application-vfs::mount_file_discovery` 的同一套 mount policy、path normalization、read/list、size limit、identity 透传和 diagnostics 基础设施。
- `agentdash-application-skill` 只保留 Skill 语义解析、frontmatter validation、duplicate name diagnostics 和 `SkillRef`/capability projection；不再复制 mount 扫描 helper。
- `agentdash-application-agentrun` 只负责编排 discovery result 到 runtime capability / context projection，不复制 VFS scanner。
- 前端 ContextFrame 列表应能稳定看到 `system_guidelines #20`，并展示 Project Guidelines section 中的 AGENTS.md 内容。

## Acceptance Criteria

- [ ] ProjectAgent 首轮启动时，工作区根存在非空 `AGENTS.md` 会生成 `system_guidelines` ContextFrame，delivery metadata 为 `session_policy #20`，model channel 为 `system`。
- [ ] `FrameLaunchEnvelope` 的 public/internal shape 完成语义分组，调用点不再需要从顶层平铺字段猜测 command/runtime/context/diagnostic 事实来源。
- [ ] `FrameLaunchEnvelope` 构造路径在最终 runtime surface 闭包后统一生成 context projection，route 层不再逐字段手填 guidelines/memory。
- [ ] ProjectAgent 后续轮或 existing surface launch 不会因为跳过 owner bootstrap 而丢失 `system_guidelines`。
- [ ] LifecycleNode launch 在 runtime VFS 可见 AGENTS.md 时同样生成 `system_guidelines`。
- [ ] Memory context 仍基于同一 launch VFS 派生，`memory_context` frame 和 bounded index 行为保持现有契约。
- [ ] VFS-first Skill provider 与 builtin Skill loader 不再复制 `should_scan_mount_for_discovery`、recursive list、read_text、size check 逻辑。
- [ ] 单元或集成测试覆盖 `AGENTS.md -> discovery result -> envelope context projection -> LaunchPlan -> system_guidelines ContextFrame` 的完整链路。
- [ ] 单元测试覆盖 Skill、Memory、Guideline adapters 共享同一 VFS discovery policy，包括 metadata allow/deny、空内容过滤和过大文件诊断。
- [ ] 前端 ContextFrame parser/render tests 覆盖 `system_guidelines` 与 identity fragments 并存时的列表展示。
- [ ] `rg` 检查确认 application-skill / application-agentrun 不再保留重复的 VFS mount scanning helpers。

## Notes

- 这不是单个 AGENTS.md bug fix，而是 runtime context discovery 的架构收束任务。实现前建议补 `design.md`，明确 discovery owner、输入输出类型、调用时机和 route 覆盖矩阵。
- 相关近期提交：`84795bb61 refactor(prompt): 收束身份上下文与系统提示词契约`、`356412a30 fix(context): 接回项目指引发现链路`。

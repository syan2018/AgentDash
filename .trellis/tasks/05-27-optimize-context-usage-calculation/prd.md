# 优化上下文计算策略

## Goal

统一 AgentDash 的上下文占用、累计消耗、本地估算三类 token 语义，让上下文状态栏、压缩触发、会话统计和前端上下文查看窗口使用同一套可验证口径。

这项改造的用户价值是让开发者在长会话中看到可信的“当前上下文占用”，理解上下文中主要信息来源，并让自动压缩基于模型实际可见的上下文压力做判断。参考项目 Codex 与 Claude Code 都将当前上下文占用和累计 token 消耗分开处理，因此 AgentDash 也应在协议、后端统计、前端展示中显式表达这种区别。

## Confirmed Facts

- Codex 的 `TokenUsageInfo` 同时包含最近一次 usage 和累计 usage；UI 的上下文剩余计算基于最近一次 active context usage。
- Claude Code 的上下文百分比基于最近一次 provider usage，并在 provider usage 缺失时使用“最后真实 usage + 后续消息估算”的混合策略。
- Claude Code 的上下文分析粒度是主分类表加二级详情：system tools、MCP tools、custom agents、memory files、skills、messages breakdown、top tools、top attachments；它不以单条消息作为默认展示粒度。
- Codex 更偏 usage/status 粒度，提供 last/total/context window 的可靠语义，但没有 Claude Code 级别的上下文构成分析。
- AgentDash 当前前端上下文环从 `usage.total.*` 提取并计算百分比，容易把累计消耗展示为当前上下文占用。
- AgentDash 后端压缩判断已倾向使用 provider-visible estimate / last input tokens，方向接近参考项目，但命名和估算口径仍分散。
- AgentDash 处于预研阶段，可以直接调整协议、类型、数据库迁移和前后端数据结构以获得正确模型。
- AgentDash 不需要命令式 `/context` 指令；上下文可观测能力应作为前端会话体验的一部分提供。

## Requirements

- 明确区分当前上下文占用、累计 session 消耗、provider usage 后新增内容估算。
- 前端上下文环和状态栏使用当前上下文占用与 effective context window 计算百分比。
- session 总用量、成本或统计类展示使用累计 session 消耗。
- 后端压缩判断使用当前 provider-visible context pressure，并统一 reserve / summary 输出预算 / effective window 口径。
- Codex bridge 保留 Codex `ThreadTokenUsage.total` 与 `ThreadTokenUsage.last` 的语义差异，并向 AgentDash 协议层传递。
- Anthropic / Claude 类 usage 归一化时，当前上下文占用优先采用 input、cache creation、cache read 相关字段。
- provider usage 尚未返回时，使用最后一次真实 usage 加后续消息估算作为实时判断依据。
- 重复的粗略 token 估算逻辑收敛到共享 helper，减少 agent streaming、compaction、projection 之间的口径漂移。
- 前端提供上下文查看窗口，用于查看当前上下文构成、主要 token 来源、压缩摘要、剩余空间与估算状态；展示粒度向 Claude Code 对齐到主分类与二级详情。
- 上下文查看窗口使用与压缩判断相同的后端上下文模型，避免 UI 另起一套解释口径。
- 上下文查看窗口不以 slash command 形式提供。
- 上下文查看窗口第一版不要求逐条消息审计，但 messages 分类需要能拆出 assistant/user、tool call、tool result、attachments，并提供 top tools / top attachments。
- 相关类型、事件、UI 文案、测试用例同步更新。

## Acceptance Criteria

- [ ] 协议层能同时表达最近一次上下文 usage、累计 usage、pending estimate、model context window、effective context window。
- [ ] 前端上下文环使用当前上下文占用计算百分比，多轮会话中的累计 token 增长不会单独推高上下文占用。
- [ ] Codex `ThreadTokenUsage.last` 驱动当前上下文展示，`ThreadTokenUsage.total` 驱动累计统计。
- [ ] Anthropic / Claude 类 usage 的当前上下文占用包含 input/cache creation/cache read 口径。
- [ ] 压缩判断基于 effective context window 与当前 provider-visible context pressure。
- [ ] provider usage 缺失时，实时状态与压缩判断可以使用本地 pending estimate。
- [ ] token 估算 helper 在后端关键路径中复用，并有针对性的单元测试覆盖。
- [ ] 前端会话界面提供上下文查看窗口，能展示当前上下文、累计消耗、pending estimate、剩余空间、主要构成分类和二级详情。
- [ ] 上下文查看窗口的分类粒度对齐 Claude Code：system/developer、tools、MCP、agents、memory、skills、messages、attachments、compaction summary、reserve、free space。
- [ ] messages 分类至少能拆分 user、assistant、tool call、tool result、attachments，并能展示 top tools / top attachments。
- [ ] 上下文查看窗口的数据来自规范化上下文模型，不重复实现独立 token 计算。
- [ ] 产品中不新增 `/context` slash command。
- [ ] 前后端测试覆盖 usage 归一化、UI 百分比、压缩阈值、projection 后 token estimate 的关键场景。
- [ ] 相关 Trellis spec 或模块文档记录新的 token 语义与架构原因。

## Planning Notes

- 本任务横跨 agent protocol、executor bridge、application hook delegate、agent compaction、frontend session model 与 UI。
- 前端上下文查看窗口纳入本任务范围，因为它会验证上下文模型是否足够解释实际会话状态。第一版粒度按 Claude Code 的主分类与二级详情对齐，不以单条消息为默认展示单元。
- 需要检查是否已有 generated TS/Rust protocol 同步流程，并把生成物纳入实施步骤。

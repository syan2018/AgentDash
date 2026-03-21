# Pi Agent 动态 Hook 上下文与伴随 Agent 机制

## Goal

为 AgentDash 的 Pi Agent 设计并持续推进一套完整的 hook runtime，使其能够逐步具备类似 Claude Code / Trellis 的伴随 agent 行为与动态上下文注入能力。

这里的目标不是简单复制某个外部产品表面，而是回答下面这些核心问题：

- Pi Agent 应该在哪些运行时阶段允许平台动态注入上下文、约束和策略？
- 如何让 session / workflow / task / story / project 的上下文在不同阶段自动切换，而不是只在 prompt 开头硬塞一次？
- 如何让伴随 agent 或 subagent 在被派发时自动获得“当前正确的上下文切片”？
- 如何把这种动态 hook 机制沉淀为平台能力，而不是 Trellis 专用硬编码？

## Background

当前 AgentDash 已经完成了第一阶段能力：

- Workflow builtin template 数据化
- workflow run / current phase 运行时解释器
- phase 级 `agent_instructions` 与 `context_bindings` 自动注入
- Project / Story / Task 会话都能感知 active workflow phase

但这仍然不等于真实的 Trellis / Claude Code 风格 hook runtime。

当前仍缺失的关键能力包括：

- SessionStart 风格的会话启动注入
- PreToolUse / PostToolUse 风格的工具前后动态介入
- subagent / companion agent 派发时的自动上下文继承
- 按当前 task / phase / runtime state 进行上下文切换
- 更细粒度的 guardrail、policy、audit、intervention 机制

## Scope

本 task 负责跟踪和推进以下能力：

- Pi Agent hook lifecycle 设计
- companion agent / subagent 行为建模
- dynamic context injection 机制
- workflow runtime 与 hook runtime 的关系划分
- hook 可观测性、调试与诊断输出
- session / turn / tool / subagent 多阶段侵入点

## Non-Goals

- 不要求短期内完整复刻 Claude Code 内部实现
- 不要求短期内实现所有 hook 类型
- 不把 Trellis 的目录结构或脚本名字直接当成平台 API
- 不要求先解决完整 automation control plane
- 不在本 task 内讨论所有 UI 产品细节

## Requirements

- 必须明确区分：
  - workflow runtime：phase 级约束与上下文解析
  - hook runtime：会话生命周期、工具生命周期、subagent 生命周期上的动态介入
- 必须支持 session 启动注入，而不只是 task 执行 prompt 注入
- 必须支持工具前后 hook，允许基于当前状态动态决定注入内容或策略
- 必须支持 companion / subagent 派发时的上下文继承与裁剪
- 必须让 hook 输出可观测、可诊断，便于解释“当前为什么注入了这些约束”
- 必须优先形成平台抽象，不把 Trellis / Claude Code 细节写死到一条实现链里

## Acceptance Criteria

- [ ] 明确 Pi Agent 的 hook lifecycle 分层与命名。
- [ ] 明确 session / turn / tool / subagent 四类 hook 的职责边界。
- [ ] 明确 companion agent / subagent 的上下文继承模型。
- [ ] 明确 workflow runtime 与 hook runtime 的协作关系。
- [ ] 明确动态注入的来源模型、解析模型与可观测模型。
- [ ] 给出逐步落地路线，至少能拆成 2 到 4 个可执行迭代。

## References

- `references/Trellis/README.md`
- `references/Trellis/.claude/hooks/session-start.py`
- `references/Trellis/.claude/hooks/inject-subagent-context.py`
- `https://docs.trytrellis.app/zh/guide/ch01-what-is-trellis`
- `https://docs.trytrellis.app/zh/guide/ch04-architecture`
- `crates/agentdash-agent/agent-design/RUST_PI_HYBRID_DESIGN.md`

## Current Judgment

当前更合理的推进方式不是“先做一套超大的 hook DSL”，而是按下面顺序逐步落地：

1. 先把 SessionStart / Subagent Dispatch 两类 hook 跑通
2. 再补 Tool 前后 hook 与 guardrail
3. 再把 workflow runtime、hook runtime、automation runtime 串成同一套状态感知体系

也就是说，hook runtime 应该是当前 workflow runtime 的上层，而不是替代品。

# Compaction 与 Session Branching Review

本目录记录 2026-05-26 对 AgentDashboard compaction、model context projection 与 session branching / lineage 模块的阶段性 review。review 目标是确认当前实现是否足够 solid、哪些边界需要在预研期收紧，以及参考实现里哪些结构值得吸收。

## 阅读顺序

1. [00-context-index.md](./00-context-index.md) - 已确认上下文、关键代码索引、subagent 分工与回传索引。
2. [01-current-implementation.md](./01-current-implementation.md) - 当前项目实现分层、数据流与证据。
3. [02-reference-comparison.md](./02-reference-comparison.md) - Codex、Claude Code、pi-mono 对照。
4. [03-findings.md](./03-findings.md) - 风险分级、solid 点、扩展性判断与建议。

## Review 边界

- 不进入 Trellis task / workflow。
- 不修改业务实现代码。
- 以代码证据为准，reference 仓库只作为设计对照。
- subagent 只做只读调研，主会话负责交叉校验和最终判断。

## 当前结论

当前项目的方向是扎实的：compaction 已经不是单纯把摘要塞进消息流，而是有 `session_compactions`、`session_projection_segments`、`session_projection_heads` 三层 checkpoint/projection/head 模型；session fork 也不是复制整段父事件，而是创建 child session、写 lineage edge，并用 `fork_initial_projection` materialize 父会话当时的 model context。

主要问题集中在语义边界和 invariant：Codex 原生 `thread/compacted` 目前不会落入 model context projection；projection commit 的 repository 校验偏弱；`POST /sessions/{id}/fork` 的 relation kind 过宽；fork point 接受裸 event seq，尚未收敛到 turn/message boundary；`branch_id` 与跨 session lineage 两套“branch”概念需要明确分层。

## 后续工作入口

1. 先处理 `03-findings.md` 中的 P1/P2：它们都是预研期越早收紧越便宜的契约问题。
2. 如果要动代码，先从 `01-current-implementation.md` 的数据流小节确认影响面。
3. 如果要做产品策略取舍，读 `02-reference-comparison.md` 中 Codex、Claude Code、pi-mono 三种分支模型的差异。

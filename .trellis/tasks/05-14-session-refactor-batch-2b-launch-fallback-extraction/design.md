# Technical Design：Batch 2b Fallback Summary

## Boundary

Batch 2b 不把所有计算都搬进 `LaunchExecution::build`。它先把“来源判定结果”类型化，避免 Batch 3 创建 `LaunchCommand` 时继续传递旧 request 的隐式字段。

## Plan Fields

新增轻量来源枚举或字符串摘要：

- `LaunchVfsSource`
- `LaunchMcpSource`
- `LaunchCapabilitySource`
- `LaunchFollowUpSource`
- `LaunchRestoreMode`

这些字段进入 `LaunchSummary`，而不是成为 connector 输入的一部分。

## What Stays In Pipeline

以下仍留在 `prompt_pipeline.rs`：

- 从 persistence 读取 meta；
- hook session reload / refresh；
- discover skills / guidelines；
- tool construction；
- context frame emission；
- connector prompt；
- stream processor supervision。

## Why This Is Enough For Batch 3

Batch 3 的 source adapter 需要知道它只表达来源意图，而不是重复解释 fallback。只要 connector 前的 summary 能完整解释 fallback 来源，入口迁移时就可以逐个 adapter 替换，而不必把 `PromptSessionRequest` 的全部字段直接复制进 `LaunchCommand`。

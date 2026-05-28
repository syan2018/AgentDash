# 优化上下文计算策略设计

## Architecture

上下文计算拆成四层：

1. provider 原始 usage：保留 Codex、Anthropic 等 provider 返回的原始结构与字段语义。
2. AgentDash usage model：归一化为当前上下文占用、累计消耗、pending estimate、窗口信息。
3. decision layer：压缩判断、状态提示、剩余空间计算统一消费 AgentDash usage model。
4. presentation layer：上下文环展示当前上下文占用，统计视图展示累计消耗，上下文查看窗口展示构成与剩余空间。

这层次化设计的原因是 provider usage 同时承载 billing、cache、当前请求和历史累计信息。将它们在协议层显式拆开，可以让 UI 与压缩逻辑选择正确语义。

## Data Contract

建议新增或调整规范化 usage 结构：

```ts
type NormalizedTokenUsage = {
  providerContextTokens: number;
  pendingEstimateTokens: number;
  currentContextTokens: number;
  cumulativeTotalTokens: number;
  modelContextWindow: number;
  effectiveContextWindow: number;
  reserveTokens: number;
  usageSource: "provider" | "provider_plus_estimate" | "local_estimate";
  last: TokenUsageBreakdown;
  total: TokenUsageBreakdown;
  segments: ContextUsageSegment[];
  details: ContextUsageDetails;
};

type TokenUsageBreakdown = {
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  cacheReadTokens: number;
  cacheCreationTokens: number;
  reasoningTokens: number;
};

type ContextUsageSegment = {
  id: string;
  kind:
    | "system"
    | "developer"
    | "memory"
    | "tools"
    | "mcp"
    | "agents"
    | "skills"
    | "messages"
    | "attachments"
    | "compaction_summary"
    | "pending_estimate"
    | "reserve"
    | "free_space";
  label: string;
  tokenEstimate: number;
  source: "provider" | "local_estimate" | "projected";
  deferred?: boolean;
};

type ContextUsageDetails = {
  systemSections: ContextUsageDetail[];
  tools: ContextUsageDetail[];
  mcpTools: ContextUsageDetail[];
  agents: ContextUsageDetail[];
  memoryFiles: ContextUsageDetail[];
  skills: ContextUsageDetail[];
  messages: MessageContextBreakdown;
};

type ContextUsageDetail = {
  id: string;
  label: string;
  tokenEstimate: number;
  source?: string;
  loaded?: boolean;
};

type MessageContextBreakdown = {
  userMessageTokens: number;
  assistantMessageTokens: number;
  toolCallTokens: number;
  toolResultTokens: number;
  attachmentTokens: number;
  toolsByType: Array<{
    name: string;
    callTokens: number;
    resultTokens: number;
  }>;
  attachmentsByType: Array<{
    name: string;
    tokens: number;
  }>;
};
```

Rust 侧可以使用同名概念建模；字段命名以 Rust 约定表达，但语义保持一致。

字段关系：

- `providerContextTokens` 是最近一次 provider usage 能确认的当前上下文占用。
- `pendingEstimateTokens` 是最近一次 provider usage 之后新增内容的本地估算。
- `currentContextTokens` 是用于 UI 百分比与压缩判断的当前压力，等于 provider usage 与 pending estimate 归一化后的最佳估计。
- `cumulativeTotalTokens` 是累计消耗，不参与上下文窗口压力判断。
- `segments` 与 `details` 解释构成；它们的估算总和可以与 provider authoritative total 存在小偏差，UI 需要标明来源。

## Provider Normalization

Codex:

- `ThreadTokenUsage.last` 表示最近一次 active context usage，可用于当前上下文展示。
- `ThreadTokenUsage.total` 表示累计 session usage，可用于统计与成本类展示。
- `model_context_window` 进入窗口字段，后续可叠加 effective window 规则。

Anthropic / Claude 类:

- 当前上下文占用使用 `input_tokens + cache_creation_input_tokens + cache_read_input_tokens`。
- output 与 reasoning 保留在 breakdown 中，用于统计和调试。
- provider usage 缺失时，使用最后真实 usage 与后续消息估算形成 pending estimate。

## Backend Decision Flow

压缩判断统一使用：

```text
context_pressure = currentContextTokens
threshold = effectiveContextWindow - reserveTokens
should_compact = context_pressure >= threshold
```

`effectiveContextWindow` 集中计算，纳入模型窗口、安全比例、summary 输出预算和固定 reserve。这样压缩逻辑关注“还能放多少 provider-visible input”，而不是关注累计消耗。

## Frontend Flow

session stream 收到 usage event 后：

1. 归一化为 `NormalizedTokenUsage`。
2. `ContextUsageRing` 使用 `currentContextTokens / effectiveContextWindow`。
3. tooltip 展示当前上下文、累计消耗、cache 相关字段。
4. usage summary card 继续使用累计消耗。

## Context Inspector

前端会话界面提供上下文查看窗口，而不是 slash command。入口可以从上下文环、状态栏或会话工具区打开。

窗口展示：

- 当前上下文占用、effective window、剩余空间、reserve。
- 累计 session 消耗，用于和当前上下文占用形成对照。
- provider usage 后新增内容的 pending estimate。
- 上下文构成分类：system/developer、memory、tools、MCP、messages、attachments、compaction summary、reserve。
- 每个分类的 token estimate、占比、数据来源。
- 二级详情对齐 Claude Code 粒度：system sections、system tools、MCP tools、agents、memory files、skills、message breakdown、top tools、top attachments。

这个窗口的核心作用是让上下文管理变得可观测。它应直接消费规范化上下文模型和 projection/compaction 统计结果，不在前端重新推导消息 token。分类精度可以随后端能力逐步增强，但字段语义必须在第一版中稳定。

第一版不按单条消息展示 token 审计。逐消息粒度会把 token 估算误差暴露为伪精确数据，也会让实现过早绑定到消息存储细节。Claude Code 的粒度证明“分类 + top contributors”已经足够支撑上下文管理决策，同时保留后续 drill-down 的扩展空间。

## Reference Alignment

Codex 对齐点：

- 保留 `last` 与 `total` 的语义差异。
- 当前上下文显示基于 `last`。
- 状态栏和窗口顶部指标使用同一套 context window 百分比。

Claude Code 对齐点：

- 主分类表覆盖 tools、MCP、agents、memory、skills、messages、free space、reserve。
- deferred / not loaded 项可展示但不计入当前上下文占用。
- messages 细分为 user、assistant、tool call、tool result、attachments。
- top tools 合并 tool call 与 tool result token。
- top attachments 按类型聚合。
- 总量优先使用 provider usage；分类估算用于解释构成。

## Feasibility Notes

当前代码已经具备实现该模型的主要来源：

- provider usage 与 context window 可从 Codex bridge 的 token usage event 进入协议层。
- 本地请求级估算可从 `BridgeRequest.system_prompt`、`messages`、`tools` 推导。
- messages breakdown 可从 `AgentMessage` 的 role、tool calls、tool result、content parts 推导。
- tool schema 粒度可从 `ToolDefinition` 推导。
- MCP server、skills、context frames 等能力信息在 session frame / hook context 中已有结构化来源。
- compaction summary 与 projection token estimate 已经持久化，可纳入构成解释。

主要不确定点不是能否实现，而是第一版分类精度。计划应允许某些分类先以 schema/request estimate 标注为 `local_estimate`，总量仍以 provider usage 为准。

## Tests

需要覆盖：

- Codex total/last 映射。
- Anthropic cache tokens 对当前上下文占用的贡献。
- pending estimate 在 provider usage 缺失时参与 context pressure。
- effective window 与 reserve 对压缩阈值的影响。
- 前端 context ring 在累计 token 增长时保持基于 current usage 的百分比。
- 上下文查看窗口展示的总量、分类占比和剩余空间来自同一份规范化 usage/context payload。
- 上下文查看窗口的二级详情覆盖 tools、MCP、memory、skills、message breakdown、top tools、top attachments。
- projection 或 compaction summary 后，当前上下文与累计消耗可以独立变化。

## Documentation

需要把新的 token 语义写入相关 spec。文档重点说明各字段存在的原因：当前上下文用于模型窗口压力，累计消耗用于统计，pending estimate 用于 provider usage 之间的实时状态，上下文分类用于帮助开发者理解模型当前能看到什么。

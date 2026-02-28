## 背景

当前 ACP 会话渲染存在以下核心问题：

- agent 回复被 `tool_call` 打断后，后续 `agent_message_chunk` 作为 continuation 到来时，UI 会新开气泡导致出现“的，”这类断头视觉问题
- 文本段与工具段的顺序/归并不可靠，`tool_call_update` 可能无法按 `toolCallId` 正确合并，出现“无限刷新/重复追加”
- 用户消息在某些情况下不显示（需要回放验证并修复）

## 目标

在不引入“奇怪兜底/过滤”的前提下，基于 ACP 原始结构与 `_meta.agentdash` 信息，重构前后端以实现：

- 同一轮（turn）内的 agent 文本合并稳定：工具调用/更新不应导致 agent 文本断头或错位
- `tool_call_update` 必须按 `toolCallId` 合并到对应 `tool_call`（或已存在的工具项）上，避免重复与无限刷新
- 用户消息必须稳定显示
- 多轮对话（多次 prompt）行为正确：每轮独立 turn，turn 边界明确

## 非目标

- 不实现 token usage 进度条、错误 UI、系统消息 UI 等未定义的渲染策略（保持 ACP 原样/不乱加工）
- 不做历史脏数据“修复性过滤”

## 设计约束/原则

- 后端：**不修改 ACP 语义**，扩展信息仅通过 `_meta.agentdash`；并保证同一轮 turnId 在“用户消息注入”和“连接器流”一致
- 前端：对未知/未支持的 ACP update 类型可以“不渲染”，但不做自作聪明的重写/过滤

## 方案概述

### 1) 后端 turnId 统一

在 `hub.start_prompt` 生成一次 `turn_id`，写入用户消息 `_meta.agentdash.trace.turnId`，并通过 `ExecutionContext` 传递给连接器；连接器创建 `NormalizedToAcpConverter` 时使用同一个 `turn_id`，使该轮所有 `agent_message_chunk` / `tool_call` / `tool_call_update` 的 `_meta.agentdash.trace.turnId` 与用户消息一致。

### 2) 前端合并器重构

- `tool_call_update`：按 `toolCallId` **从尾到头回扫**合并到最近的工具项（匹配 ABCCopilot 的稳定策略）
- 文本 chunk：基于 `_meta.agentdash.trace.turnId` 做“同 turn 的文本合并”，避免因工具插入导致断头；同时保持时间线渲染稳定

## 验收标准（回放 + GUI）

- 用 `.agentdash/sessions/*.jsonl` 回放：同一轮中出现 `tool_call` 夹在 agent 文本中间时，最终 agent 文本显示为连贯句子，不出现孤立“的，”开头的新气泡
- `tool_call_update` 不产生重复工具项；同一个工具调用在 UI 中仅一条记录，状态/输出随 update 更新
- 连续 3 轮 prompt：每轮都有用户消息、agent 回复、工具调用（如果有）且归并正确，上一轮不被后续轮污染


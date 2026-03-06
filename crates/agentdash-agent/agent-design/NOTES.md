# Rig vs Pi agent 设计调研

调研时间：2026-03-06

本地仓库：
- `rig` @ `ac9033a6`
- `pi-mono` @ `b14c3592`

## 一句话结论

- **Rig** 更像一个“Rust 原生的 agent 组件库 / workflow primitives”，优点是类型系统强、provider/向量库抽象统一、RAG 和结构化输出一体化、适合把 agent 深度嵌入后端系统。
- **Pi** 更像一个“分层的 agent runtime + coding-agent 产品架构”，优点是事件流清晰、状态和会话管理完整、扩展点非常多、工程化体验成熟，特别适合做交互式 agent 产品或自定义工作流。

## Rig 的 agent 设计优点

### 1. 以 Rust trait 为中心，核心抽象稳定且统一

Rig 的设计不是先做一个“黑盒 agent runtime”，而是先统一底层能力：`CompletionModel`、`EmbeddingModel`、`Tool`、`VectorStoreIndex`。Agent 只是这些能力之上的组合层。

这带来的优点：
- provider、embedding、tool、vector store 都能走同一套类型系统，而不是每接一个新模型就裂出一层新 API。
- agent 能天然复用 completion / prompt / chat / streaming 等能力，而不是单独维护一套 agent-only 协议。
- 对 Rust 项目很友好：抽象边界清晰，便于做库级复用和静态约束。

相关位置：
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\AGENTS.md:32`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\src\agent\mod.rs:1`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\src\agent\completion.rs:160`

### 2. Builder 模式让 agent 装配非常自然

Rig 的 agent 是一个组合产物：模型 + preamble + static context + dynamic context + tools + schema + params。`AgentBuilder` 把这些拼装过程做成 fluent API。

优点：
- 配置路径很短，适合业务代码内联装配。
- 静态上下文、动态上下文、工具、输出 schema 都在一条 builder 链上，认知负担低。
- 对于 Rust 使用者，builder 比“大而全配置对象”更符合生态习惯，也更利于泛型扩展。

相关位置：
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\src\agent\builder.rs:77`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\src\agent\builder.rs:130`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\src\agent\builder.rs:159`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\src\agent\builder.rs:201`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\src\agent\builder.rs:472`

### 3. 把 RAG / tools / structured output 融进同一个请求构建过程

Rig 的亮点不是“有 agent”，而是把 agent 的三个常见能力统一到同一条 completion request pipeline 里：
- 静态文档上下文
- 动态上下文检索（`dynamic_context`）
- tool calling
- 结构化输出 schema

这意味着：
- 你不需要在应用层自己拼很多中间层，把 retrieval、tool defs、输出约束塞进不同通道。
- agent 的“知识增强”和“行动能力”是同等公民，不是后挂插件。
- 很适合构建强约束的业务 agent，比如提取、分类、RAG 问答、工作流节点。

相关位置：
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\src\agent\completion.rs:160`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\src\agent\completion.rs:206`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\src\agent\mod.rs:69`

### 4. PromptRequest / StreamingPromptRequest 把多轮工具循环显式化

Rig 不是只暴露 `agent.prompt()` 这种最上层接口，它还把 request object 单独建模出来，并支持：
- `max_turns`
- `with_hook`
- streaming 多轮工具执行
- typed prompt / typed response

优点：
- “一次 prompt 背后会发生多轮 tool loop” 这件事被显式建模了，而不是藏进黑盒。
- 调试和定制空间更大，尤其适合需要拦截 tool call、注入策略、追踪 streaming 的后端服务。
- typed request/response 很适合 Rust 里把 agent 输出接进后续业务逻辑。

相关位置：
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\src\agent\prompt_request\mod.rs:47`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\src\agent\prompt_request\mod.rs:158`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\src\agent\prompt_request\mod.rs:178`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\src\agent\prompt_request\streaming.rs:175`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\src\agent\prompt_request\streaming.rs:289`

### 5. Hook 机制让治理、审计、护栏更容易落地

Rig 在 tool call 前后、stream delta 过程中都提供了 hook 点，而且 hook 可以决定 continue / skip / terminate。

优点：
- 很适合做安全护栏、工具白名单、审计、超时中止、人工审批等“治理层”能力。
- 因为 hook 是围绕请求与 tool lifecycle 建模的，所以不会强绑定某个 UI 或运行环境。
- 对服务端 agent 来说，这比单纯 callback 更利于形成统一中间件。

相关位置：
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\src\agent\prompt_request\hooks.rs:12`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\src\agent\prompt_request\hooks.rs:39`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\src\agent\prompt_request\hooks.rs:96`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\src\agent\prompt_request\hooks.rs:128`

### 6. Workflow primitives 明显偏“可编排”而不是“单体 Agent”

Rig 仓库里专门给了 orchestrator、parallelization、prompt chaining、routing、evaluator-optimizer 等示例。这说明它的思路是：agent 不只是聊天体，而是工作流中的一个可组合算子。

优点：
- 更适合复杂任务拆解、多 agent 协作、并行评估、路由等编排场景。
- 和 Rust 的 pipeline / typed ops 风格比较一致，适合做后端工作流引擎。

相关位置：
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\examples\agent_orchestrator.rs:1`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\examples\agent_parallelization.rs:1`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\rig\rig\rig-core\examples\agent_prompt_chaining.rs:1`

## Pi 的 agent 设计优点

### 1. 分层非常清楚：provider 层、agent runtime 层、product 层分离

Pi monorepo 很值得借鉴的一点是分层：
- `packages/ai`：统一 provider / model / streaming event
- `packages/agent`：状态化 agent loop、tool execution、事件流
- `packages/coding-agent`：CLI、session、skills、extensions、UI、持久化

优点：
- 下层足够通用，上层足够产品化。
- 你既可以只用 `pi-ai` / `pi-agent-core` 自己做产品，也可以直接用 `coding-agent` 成品。
- 各层职责边界比很多“所有逻辑塞进一个 agent class”的项目更清晰。

相关位置：
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\README.md:22`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\ai\README.md:1`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\agent\README.md:1`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\coding-agent\README.md:372`

### 2. AgentMessage 与 LLM Message 分离，扩展性特别强

Pi 在 `packages/agent` 里专门区分了：
- `AgentMessage`：应用内部消息，可包含自定义类型
- `Message`：真正发给模型的 LLM 消息

并通过：
- `transformContext()`
- `convertToLlm()`

把二者桥接起来。

这是 Pi 设计里最值得学的一点之一。优点是：
- 应用层可以有 UI-only message、notification、artifact、review note 等消息，不强迫它们都伪装成 user/assistant/toolResult。
- 上下文裁剪、摘要注入、外部状态注入可以在 `transformContext` 做。
- 真正送进 LLM 的消息在 `convertToLlm` 做最后投影，边界清晰。

相关位置：
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\agent\README.md:34`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\agent\src\types.ts:22`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\agent\src\types.ts:48`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\agent\src\types.ts:67`

### 3. 事件流设计很完整，天然适合 UI 和可观测性

Pi 的 agent loop 把生命周期拆成：
- `agent_start` / `agent_end`
- `turn_start` / `turn_end`
- `message_start` / `message_update` / `message_end`
- `tool_execution_start` / `tool_execution_update` / `tool_execution_end`

优点：
- UI 很容易做实时更新，而不需要猜状态。
- 日志、回放、指标、调试都很直观。
- 这套事件流不仅适用于 CLI，也适用于 web / Slack / 代理服务。

相关位置：
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\agent\README.md:54`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\agent\src\types.ts:177`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\agent\src\agent-loop.ts:28`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\agent\src\agent-loop.ts:204`

### 4. Tool loop、steering、follow-up 被作为一等能力显式建模

Pi 不只处理“模型调工具再继续”这条经典回路，还把运行中用户插话和排队消息也纳入 agent loop：
- `getSteeringMessages()`：执行中打断 / 转向
- `getFollowUpMessages()`：当前任务结束后再继续处理
- `steeringMode` / `followUpMode`：控制一次送一条还是批量送

优点：
- 更贴近真实交互式 agent 产品，而不是纯函数式单次推理。
- 适合长任务、工具执行耗时长、用户会中途改主意的场景。
- 比很多只在 UI 层 hack interrupt 的方案更系统。

相关位置：
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\agent\src\types.ts:80`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\agent\src\types.ts:92`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\agent\src\agent.ts:115`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\agent\src\agent-loop.ts:104`

### 5. 会话持久化、分支、压缩是产品级设计，不是事后补丁

`pi-coding-agent` 不是只会跑 agent loop，它把 session 设计成 JSONL tree，可继续、分叉、压缩、回看。

优点：
- 很适合 coding agent、research agent 这种长对话、长任务场景。
- 分支不是“另存一份聊天记录”，而是 session tree 里的原生能力。
- compaction 是 runtime 级恢复手段，不只是“上下文太长时做个 summary”。

相关位置：
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\coding-agent\README.md:193`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\coding-agent\README.md:208`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\coding-agent\README.md:220`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\coding-agent\src\core\agent-session.ts:2`

### 6. “极简核心 + 强扩展面”让产品演化更灵活

Pi 在 README 里明确强调：核心不内置 MCP、sub-agents、permission popups、plan mode、todo system，而是通过 skills / extensions / packages 自行扩展。

这套设计的优点是：
- 核心运行时更小、更稳定，不容易被某种 workflow 绑死。
- 产品需求变化时，可以优先在 extension 层试验，而不是反复污染核心 loop。
- 对团队来说，便于做企业内定制：权限、审计、UI、工具、MCP、沙箱都能外挂。

相关位置：
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\coding-agent\README.md:402`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\coding-agent\README.md:260`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\coding-agent\README.md:274`

### 7. `pi-ai` 的 provider 归一化做得很适合上层 agent 复用

`pi-ai` 把多 provider 的：
- streaming event
- thinking/reasoning
- tool call partial JSON
- stop reason
- token/cost 统计
- cross-provider handoff

先做了统一。

优点：
- 上层 agent runtime 不必为不同厂商写不同 tool loop。
- 切模型、跨 provider 续上下文、做成本统计都更容易。
- 这让 `pi-agent-core` 和 `pi-coding-agent` 可以专注状态管理，而不是 provider 兼容细节。

相关位置：
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\ai\README.md:1`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\ai\README.md:364`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\ai\README.md:419`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\pi-mono\packages\ai\README.md:781`

## 两者设计取向的核心差异

### Rig 更偏“库型 agent 框架”

更适合：
- Rust 后端服务
- 需要强类型、结构化输出、RAG、workflow 编排
- 希望把 agent 嵌入现有服务，而不是直接拿一个完整交互产品

它的优势关键词：
- trait-based abstraction
- typed builder
- RAG/tool/schema 一体化
- multi-agent workflow primitives
- hook-based governance

### Pi 更偏“运行时 / 产品型 agent 架构”

更适合：
- coding agent / desktop agent / Slack agent / web chat agent
- 需要会话、回放、分支、压缩、UI streaming
- 需要大规模定制工具、权限、交互行为、扩展机制

它的优势关键词：
- layered architecture
- event-driven runtime
- stateful session management
- steering/follow-up queues
- extensions/skills/packages ecosystem

## 如果你要选型

- **选 Rig**：当你的主战场是 Rust，且你更想获得“agent primitives + workflow 编排能力”，自己把它嵌到业务系统里。
- **选 Pi**：当你要做的是“一个可长期交互、可扩展、可持久化的 agent 产品”，尤其是 coding / ops / assistant 这类应用。
- **折中理解**：Rig 更像“强类型 agent SDK”；Pi 更像“agent runtime + product architecture reference”。

## 我个人认为最值得借鉴的点

- 从 **Rig** 借鉴：trait 抽象、builder 体验、typed structured output、hook 治理、workflow primitives。
- 从 **Pi** 借鉴：AgentMessage 与 LLM Message 分层、事件驱动生命周期、会话树与 compaction、steering/follow-up、扩展优先的产品架构。


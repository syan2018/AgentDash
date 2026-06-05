# 研究：发送/Steer 链路协议现状 + 双模追加消息 gap

## 1. ContentBlock vs codex UserInput —— 后端真实存活情况

**结论：ContentBlock 并非"前端忘删的残留"，它在后端仍是 message 链路的承载类型，清理它是一次真实重构（中等量），不是删几行前端。**

证据：
- `agentdash-spi` 的连接器入口 `PromptPayload` 仍是 `Text(String) | Blocks(Vec<ContentBlock>)`（[connector/mod.rs:500](../../../../crates/agentdash-spi/src/connector/mod.rs)）。`prompt()` 直接收 `&PromptPayload`。
- message API：`prompt_blocks: Vec<JsonValue>` → `UserPromptInput::resolve_prompt_payload` 逐个 `serde_json::from_value::<ContentBlock>` → `PromptPayload::Blocks`（[session/types.rs:179](../../../../crates/agentdash-application/src/session/types.rs)）。
- 转 codex 仅发生在持久化/事件侧：`commit.rs` 调 `content_blocks_to_codex_user_input(user_blocks)`（[user_input.rs:58](../../../../crates/agentdash-agent-protocol/src/backbone/user_input.rs)）。
- steer 链路：API `input: Vec<codex::UserInput>` → `steer_session(_, _, Vec<codex::UserInput>)`，**全程 codex，不碰 ContentBlock**。

真正的 canonical 边界形态：**codex `UserInput`**（steer 入口 + 持久化 `UserInputSubmittedNotification.content` 都用它）。ContentBlock 是叠在其上的、仅 message 链路使用的前端方言。

→ 用户判断方向成立（codex UserInput 才是真值），但"杀干净"尚未完成。要统一需改：`PromptPayload`（去 `Blocks` 或改承载 codex UserInput）、`resolve_prompt_payload`、`commit.rs`、`content_block_to_text`、message API 入参契约 + 重新生成 TS。属真重构。

## 2. 后端三个投递原语

| 原语 | 语义 | 入参 | 是否有 HTTP route |
|---|---|---|---|
| `prompt` | 起**新 turn** | `PromptPayload`(ContentBlock) | 有（messages） |
| `steer_session` | 注入**运行中 turn**（不建新 turn） | `Vec<codex::UserInput>` | 有（steering-messages） |
| `push_session_notification` | 塞进 steering 队列，**下次 LLM 调用前合并到对话末尾**（KV-cache 稳定，不打断） | `String` | **无**（仅内部用于"phase changed"等 out-of-band 通知，pi_agent/composite 实现） |

## 3. 当前 action 可用性模型（runtime-control）

来自 [routes/sessions.rs:244-301](../../../../crates/agentdash-api/src/routes/sessions.rs)，`delivery_running = last_delivery_status == Running`：
- `send_next`：`has_frame && !terminal && !delivery_running` —— **仅 idle**
- `steer`：`has_frame && !terminal && delivery_running && supports_steering` —— **仅 running+可steer**
- `cancel`：`delivery_running`

→ send_next 与 steer **被 `delivery_running` 互斥**；运行中 send_next 显式禁用（"不能并发发送下一轮消息"）。前端 `chatControlState` 据此只挑一个 `primaryAction`（steer 优先，否则 send_next，否则 none），UI 上是**单一主操作**。

## 4. 双模追加消息（pending 自动接续 / 直接 steer）—— GAP 清单

目标交互（参考 codex/@references）：运行中输入消息时，用户可选 (a) **排队**：等当前 turn 完成自动作为下一轮接续；(b) **steer**：立即注入运行中 turn。

- **G1 互斥的 action 模型**：现模型把 running 态唯一出口定为 steer，send_next 硬禁。要双模需让 running 态同时暴露「排队」与「steer」两个可用动作（拆分按钮 / 模式切换），而非自动二选一。
- **G2 缺「排队为下一轮」语义**：`push_session_notification` 是"合并进当前轮"，**不是**"完成后起新 turn"。两种 pending 语义需明确选型：
  - (a) merge-into-current（已有原语，缺 route）——接近"无打断软注入"。
  - (b) queue-as-next-turn（turn_completed 后自动发 send_next）——**无后端原语**；可前端编排（暂存消息 + 监听 `turn_completed` 事件后调 messages），或新增后端 pending-turn 队列。
- **G3 排队消息无可见性/持久化**：codex 会展示"已排队"消息；现无对应 event/state，刷新/多端不同步。前端编排方案下排队消息仅存在本地内存。
- **G4 payload 能力不对称**：send_next 携带 `prompt_blocks + executor_config`（可换模型/轮次配置）；steer 仅 `input`（无 executor_config）。→ 排队（走新 turn）可换模型，steer 不可。与"内联模型选择器"功能相关：steer 时模型选择器应只读/隐藏。
- **G5 capability 感知**：steer 需 `supports_steering`；不支持时双模应自动退化为仅排队。UI 需按 `runtimeControl.actions.steer.enabled` 决定是否给 steer 选项。
- **G6 与图片/ContentBlock 统一耦合**：若按 §1 收敛到 codex UserInput，则排队(新 turn) 与 steer 的 input 终于同形，前端单 builder；否则排队走 ContentBlock、steer 走 UserInput，双模会放大现有方言分裂。→ 协议统一是双模的良好前置。

## 5. 建议的最小可行选型（待用户确认）

- pending 语义选 **(b) queue-as-next-turn**，先做**前端编排**（暂存 + turn_completed 自动发），G3 的持久化作为后续增强；理由：贴合用户"等轮次完成自动接续"的描述，且不需立刻动后端队列。
- 协议统一（§1）作为双模与图片的共同前置，方向 = codex UserInput。

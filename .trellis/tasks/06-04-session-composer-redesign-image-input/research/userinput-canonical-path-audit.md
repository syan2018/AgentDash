# 研究：UserInput / ContentBlock / text 转换乱象审计 + 一致路径目标

## 头号结论（影响图片功能可行性）

**图片在 pi_agent 全链路都被拍平成文本，模型根本看不到图。** 协议层"支持"`ContentBlock::Image` / `UserInput::Image` 只是类型存在，实际投递时三条路径都 flatten 成 `"[引用图片: ...]"` 文本：

| 路径 | flatten 点 | 结果 |
|---|---|---|
| prompt（新 turn） | `pi_agent.prompt()` 第一行 `prompt.to_fallback_text()`（[connector.rs:568](../../../../crates/agentdash-executor/src/connectors/pi_agent/connector.rs)） | 图片 → `content_block_to_text` 的 `"[引用图片: mimeType=...]"` 文本 |
| steer | `pi_agent.steer_session()` → `codex_user_input_to_text(&input)`（[connector.rs:891](../../../../crates/agentdash-executor/src/connectors/pi_agent/connector.rs)） | 图片 → url 字符串当文本 |
| continuation（冷启动重建） | `codex_user_input_to_message_part()` Image 分支（[continuation.rs:455](../../../../crates/agentdash-application/src/session/continuation.rs)） | 图片 → `"[引用图片: {url}]"` 文本 |

而模型层 `ContentPart::Image { mime_type, data }` **本就存在**（[content.rs:11](../../../../crates/agentdash-agent-types/src/model/content.rs)，注释明说"支持 Text/Image/Reasoning，覆盖主流 LLM"），只是：
- 没有 `ContentPart::image()` 构造器；
- **没有任何 user-input 转换会产出 `ContentPart::Image`**，全部走 `ContentPart::text`。

→ **图片输入功能要真正生效，必须把 user-input 投递路径打通到 `ContentPart::Image`，而不仅是前端发 image block。否则功能是装饰性的——模型只收到一行占位文本。** 这使"协议清理/一致路径"从 nice-to-have 变成图片功能的**硬前置**。

## 转换器乱象（≥4 个平行的有损 flattener，各自格式化图片）

1. `codex_user_input_to_text`（protocol [user_input.rs:109](../../../../crates/agentdash-agent-protocol/src/backbone/user_input.rs)）：UserInput→text，Image=url、LocalImage=path、Skill=name、Mention=name。
2. `content_block_to_text`（spi [connector/mod.rs:505](../../../../crates/agentdash-spi/src/connector/mod.rs)）：ContentBlock→text，Image=`[引用图片: mimeType=...]`、Audio=`[引用音频...]`、Resource/ResourceLink=各自文本。
3. `codex_user_input_to_message_part`（continuation [continuation.rs:443](../../../../crates/agentdash-application/src/session/continuation.rs)）：UserInput→ContentPart，又一套图片/skill/mention 文本格式。
4. `content_block_to_codex_user_input`（protocol [user_input.rs:71](../../../../crates/agentdash-agent-protocol/src/backbone/user_input.rs)）：ContentBlock→codex UserInput，Image 拼 data URL、Audio/Resource→text。

外加 compat/mod.rs（自标"P0.4 完成后移除"的过渡层）里 ContentBlock↔codex UserInput 的双向有损 round-trip：
- `UserInputSubmitted` 事件 → `codex_user_input_to_text` 拍平存文本，失败再 fallback `serde_json::to_string`（[compat/mod.rs:90](../../../../crates/agentdash-agent-protocol/src/compat/mod.rs)）。
- 反向 `content_block_to_codex_user_input(&chunk.content)`：chunk 只带**单个** ContentBlock，而 UserInput 是 `Vec`，失败 fallback 把整块 JSON 当 Text 塞（[compat/mod.rs:432](../../../../crates/agentdash-agent-protocol/src/compat/mod.rs)）。

这正是 [[feedback_no_hacks_in_connectors]] 说的"同类拼接/字段绕过散落各 adapter 各自玩"。

## 三种表示并存

- ACP `ContentBlock`（message API 入参 `prompt_blocks` + `PromptPayload::Blocks` + relay 边界 + ACP 事件）
- codex `UserInput`（steer API 入参 + 持久化 `UserInputSubmittedNotification.content`）
- `ContentPart`（模型层 / AgentMessage，真正喂给 LLM；唯一能携带真图片的）
- 派生 text（各 flattener 的归宿）

## 建议的一致路径目标（待用户拍板）

**单一 canonical user-input 表示贯穿"API 入参 → 应用层 → 连接器 → AgentMessage"，图片以结构化形式直达 `ContentPart::Image`，不再中途拍平。**

候选 canonical：codex `UserInput`（用户倾向，且已是 steer + 持久化的事实真值）。则：
- message API 与 steer API 统一收 `Vec<codex::UserInput>`；删除 `prompt_blocks`(ContentBlock) 入参与 `PromptPayload::Blocks`（或令 `PromptPayload` 直接承载 `Vec<UserInput>`）。
- 连接器侧新增**唯一**的 `UserInput → Vec<ContentPart>` 映射（Image→`ContentPart::Image{mime_type,data}`，Text→Text，Mention/Skill/LocalImage→定义好的结构或文本），替换掉 prompt/steer/continuation 三处各自的 flatten。
- ContentBlock 仅保留在 ACP relay 边界（远程后端互通），并集中到一处转换；删除散落的 `content_block_to_text` / `codex_user_input_to_message_part` / `codex_user_input_to_text` 重复实现，保留至多一个"仅用于标题/trace 摘要"的 text 投影。
- compat/mod.rs 按其自身 TODO 评估能否随 P0.4 退场。

## 风险 / 注意

- `codex::UserInput` 来自外部 crate `codex_app_server_protocol`，其变体（Text/Image/LocalImage/Skill/Mention）是否够表达本项目所有输入（如 ACP Resource/ResourceLink 文件引用）需确认；不够则需在边界定义映射或扩展自有类型。
- 持久化事件 `UserInputSubmittedNotification.content: Vec<UserInput>` 已是 codex，统一方向与持久化一致 → 迁移成本低。
- 跨 crate 改动面：agent-protocol / spi / application(types,commit,continuation) / executor(pi_agent) / api routes + 重新生成 TS 契约。

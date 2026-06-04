# [child-1] 技术设计：canonical 用户输入路径

参见 parent [design.md](../06-04-session-composer-redesign-image-input/design.md) 的目标形态图与跨 child 契约。

## 0. 对齐 Codex 实际输入策略（收口依据）

我们 pin 的是 codex `rust-v0.133.0`（rev 9474e5c）。其 app-server v2 输入模型（`codex_app_server_protocol::UserInput`，即我们 `use ... as codex` 绑定的类型）：

- `Text { text, text_elements: Vec<TextElement> }` — `text_elements` 是 **byte-range 标记**（图片占位、mention 等），用于在 text 上渲染/持久化富元素**而不污染字面文本**，跨 history/resume 稳定。
- `Image { detail?, url }`（data URL）、`LocalImage { detail?, path }`（序列化时转 Image data URL）、`Skill { name, path }`、`Mention { name, path }`（`path` 形如 `app://...` / `plugin://...`）。

Codex 关键事实（决定我们怎么收口）：
1. **`turn/start` 与 `turn/steer` 用同一个 `Vec<UserInput>`**。即"消息"与"steer"在 Codex 本就是同形入参——我们项目 message 走 ContentBlock 是**自己引入的分裂**，不是 Codex 的。→ 收口 = message 也收 `Vec<UserInput>`。
2. 我们的 `steer_session(session_id, expected_turn_id, input)` **已镜像** Codex `TurnSteerParams{ thread_id, input, expected_turn_id }`（含 expected_turn_id 前置校验）。方向正确，沿用。
3. `turn/interrupt` = 我们的 cancel。Codex **没有** server 端 "queue-next-turn" 原语——只有 start/steer/interrupt；排队是其上的编排层（→ child-4 服务端托管的依据）。

**收口策略**：canonical = codex v2 `UserInput`（用 `UserInputBlock` 封名留接缝）；前端按 Codex 语义产出结构化变体，弃 `<file:path>` 文本标记 hack：
- `@` 文件/连接器引用 → `Mention { name, path }`（path 用项目既有 uri，如 `file://`/`app://`）。
- slash 命令/Skill → `Skill { name, path }`。
- 粘贴/拖拽图片 → `Image { url: data URL }`（来自磁盘路径的可用 `LocalImage`）。
- 输入框内的可视标记（药丸、图片占位）→ `Text.text_elements` 的 byte-range，而非往文本里塞 `<file:...>`。

## 接缝类型（R1）

在 `crates/agentdash-agent-protocol`：
```rust
// 当前：别名直接复用 codex。后续转自定义扩展时只改此处 + 映射实现。
pub type UserInputBlock = codex_app_server_protocol::UserInput;
```
- 全项目改为 `use agentdash_agent_protocol::UserInputBlock`（或 re-export 路径）。
- 若后续要扩展（如 Resource/ResourceLink 文件引用），将 `type` 升级为本地 `enum`，并在边界补 `From/Into codex::UserInput`。本 child 不实现 enum，仅确立命名与单一引用点。

## 唯一映射（R2）

新增单一函数（建议落在 `agent-protocol` 或 `agent-types` 边界，靠近 `ContentPart`）：
```rust
pub fn user_input_blocks_to_content_parts(input: &[UserInputBlock]) -> Vec<ContentPart>
//  Text     -> ContentPart::Text{text}
//  Image{url}      -> 解析 data URL / 远程 url -> ContentPart::Image{mime_type,data}
//  LocalImage{path}-> 读取/或保留路径语义（MVP：读不到则降级文本，并 log）
//  Skill/Mention   -> ContentPart::Text（保留现有 "[引用...]" 语义，集中到此处唯一定义）
```
- 给 `ContentPart` 增 `image(mime_type, data)` 构造器。
- data URL 解析：`data:<mime>;base64,<data>` → 拆 `mime_type` + `data`；非 data URL 的远程 url 策略在 design 决议（MVP 可保留为文本占位并 log，图片采集侧 child-2 保证传 data URL）。

替换点：
- `pi_agent.prompt()`：不再 `to_fallback_text()`；改由 `PromptPayload`(承载 `Vec<UserInputBlock>`) → `user_input_blocks_to_content_parts` → `AgentMessage::user_parts(...)`。
- `pi_agent.steer_session()`：`input: Vec<UserInputBlock>` → 同映射 → `AgentMessage`。
- `continuation::codex_user_input_to_message_part` 删除，调用点改用统一映射。

## API / PromptPayload（R3）

- `PromptPayload`：`Text(String) | Blocks(Vec<ContentBlock>)` → `Text(String) | Input(Vec<UserInputBlock>)`（或直接 `Input(Vec<UserInputBlock>)` 单变体 + text 便捷构造）。`to_fallback_text` 保留（标题/trace 摘要用），但不再是投递路径。
- `UserPromptInput`（session/types.rs）：`prompt_blocks` → `input: Vec<UserInputBlock>`；`resolve_prompt_payload` 不再 `from_value::<ContentBlock>`。
- message API route + `LifecycleAgentMessageCommand/Delivery`：`prompt_blocks: Vec<serde_json::Value>` → `input: Vec<UserInputBlock>`。
- `commit.rs`：删 `content_blocks_to_codex_user_input(user_blocks)`，直接用 `Vec<UserInputBlock>`。
- TS：`generate_ts` 重新生成；前端 `sendLifecycleAgentMessageByRuntimeSession` 请求体字段从 `prompt_blocks` → `input`（child-1 内做最小适配，保证现有文本发送不回归；完整前端在 child-2/3）。

## ContentBlock 收敛（R4）

- 保留 `ContentBlock <-> UserInputBlock` **各一个方向**、单实现，仅供 relay/远程边界（`relay_connector.rs`、compat）。
- 删除：`codex_user_input_to_text`（protocol）、`content_block_to_text`（spi，若仅服务 fallback 文本则并入 `to_fallback_text`）、`codex_user_input_to_message_part`（continuation）。
- compat/mod.rs：评估 round-trip 是否随 P0.4 退场；本 child 至少把其依赖的转换指向统一实现，给出退场结论。

## 改动文件清单（逐文件，含回滚点）

| Crate | 文件 | 改动 |
|---|---|---|
| agent-protocol | `backbone/user_input.rs`, `lib.rs` | 定义 `UserInputBlock`；新增/迁移唯一映射；删 `codex_user_input_to_text` |
| agent-types | `model/content.rs` | `ContentPart::image()` 构造器 |
| spi | `connector/mod.rs` | `PromptPayload` 改形；`content_block_to_text` 处置 |
| application | `session/types.rs` | `UserPromptInput.input` + `resolve_prompt_payload` |
| application | `session/launch/commit.rs` | 去 ContentBlock 转换 |
| application | `session/continuation.rs` | 删 `codex_user_input_to_message_part`，用统一映射 |
| application | `workflow/agent_message.rs` | command/delivery `input` 字段 |
| application | `relay_connector.rs` | relay 边界 ContentBlock 适配 |
| executor | `pi_agent/connector.rs` | `prompt`/`steer_session` 走统一映射 |
| api | `routes/lifecycle_agents.rs`, contracts | API 入参 + TS 重生成 |

回滚点：接缝类型与映射函数为纯增量，先落地 + 单测；API/PromptPayload 改形为一组原子提交；relay/compat 适配单独提交。任一组失败可回退该组而不动其余。

## 风险

- `codex::UserInput` 变体不足以表达文件引用（Resource/ResourceLink）；本 child 保持现有"文本标记"语义集中到唯一映射，扩展留给后续自定义类型。
- relay 远程后端仍可能期望 ACP ContentBlock；边界转换必须保真往返（单测覆盖）。
- 跨 crate 大改，需保证既有测试（hub/tests.rs 中 `PromptPayload::Blocks` 断言等）同步更新。

## 测试策略

- 单测：`user_input_blocks_to_content_parts`（text/image/mention 各态，image 真出 `ContentPart::Image`）；data URL 解析；relay 边界 `ContentBlock↔UserInputBlock` 往返。
- 回归：pi_agent prompt/steer 用带 image 的 input，断言产出 `AgentMessage` 含 image part（非占位文本）。
- 既有：更新 hub/tests.rs、connector/mod.rs tests 中针对 `Blocks`/`to_fallback_text` 的断言。

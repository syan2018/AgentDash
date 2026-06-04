# [Parent] 跨 child 架构：canonical 用户输入路径

本文件只描述**跨 child 的共同架构与契约**；各 child 的落地细节在其自己的 design.md。

## 目标形态

```
前端 composer
   │  单一 builder：text + 附件 → Vec<UserInputBlock>
   ▼
API 入参（message & steer 统一）  Vec<UserInputBlock>
   │
应用层（types/commit/continuation）  不再拍平，透传结构
   ▼
连接器边界  唯一映射 UserInputBlock → Vec<ContentPart>
   │        （Image → ContentPart::Image{mime_type,data}）
   ▼
AgentMessage(ContentPart) → LLM（真多模态）
```

- `UserInputBlock` = 本项目 protocol crate 内对 `codex::UserInput` 的别名/newtype（接缝层）。调用方一律用此名。
- ACP `ContentBlock` 退到 relay/远程边界，转换集中一处（`ContentBlock <-> UserInputBlock` 各一个方向，单实现）。

## 待消灭/合并的现状（child-1 负责）

- `codex_user_input_to_text`（protocol）
- `content_block_to_text`(spi) + `PromptPayload::Blocks`
- `codex_user_input_to_message_part`（continuation）
- `content_block_to_codex_user_input` / `content_blocks_to_codex_user_input`（protocol）
- compat/mod.rs 的 ContentBlock↔codex round-trip（按其自身 P0.4 TODO 评估退场）

目标：连接器侧只保留**一个** `UserInputBlock → Vec<ContentPart>` 映射；text 投影至多保留一个、且仅用于标题/trace 摘要（明确标注用途）。

## 契约影响（child-1 产出，child-2/3/4 依赖）

- message API：`prompt_blocks: Vec<JsonValue>(ContentBlock)` → `input: Vec<UserInputBlock>`（与 steer 同形）。
- steer API：维持 `input: Vec<UserInputBlock>`。
- `PromptPayload`：去 `Blocks(Vec<ContentBlock>)`，改承载 `Vec<UserInputBlock>`（或等价）。
- 重新生成 TS：前端 `UserInput` 类型成为唯一发送形态；`acp.ts` 的 ContentBlock 发送用途退役（展示用途另议）。

## 兼容性 / 迁移

- 持久化 `UserInputSubmittedNotification.content: Vec<UserInput>` 已是 codex，方向一致，迁移成本低。
- 跨 crate 改动面：agent-protocol / spi / application(types,launch/commit,continuation) / executor(pi_agent) / api routes。child-1 design.md 给逐文件清单与回滚点。
- 远程 relay 路径（`relay_connector.rs` 把 `PromptPayload::Blocks` 转 json）需同步适配，是 child-1 的兼容必检项。

## 各 child 与本架构的接点

- **child-1**：实现上图全部（接缝类型、唯一映射、API 统一、删冗余、TS 重生成）。
- **child-2**：前端单 builder 产出含 image 的 `Vec<UserInputBlock>`；验证 `ContentPart::Image` 真达模型。
- **child-3**：composer 重构；内联模型/推理选择器只改 `executor_config`，与 input 表示正交。
- **child-4**：running 态双出口。**排队 = 服务端托管完整状态**（pending 队列领域实体 + 服务端在 turn_completed 自动派发新 turn + 事件投影），前端 UI 是投影；steer = 注入（复用 child-1 后统一的 `Vec<UserInputBlock>` 入参，镜像 Codex turn/steer）。排队走新 turn（带 executor_config，可换模型），steer 仅 input。Codex 仅有 start/steer/interrupt 原语，排队是其上的服务端编排层。

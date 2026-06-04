# [child-1] 用户输入链路一致化（canonical UserInput → ContentPart）

Parent: [06-04-session-composer-redesign-image-input](../06-04-session-composer-redesign-image-input/prd.md)
依赖：无（本 child 是其余 child 的硬前置）。

## Goal

把后端"用户输入"收敛到单一 canonical 表示，贯穿 API→应用→连接器→AgentMessage，**让多模态（图片）结构化直达 `ContentPart::Image`**，并删除散落的有损 text flattener。为 child-2/3/4 提供统一契约。

## 背景（见 parent research/）

- 图片在 pi_agent `prompt`/`steer`/`continuation` 三路被拍平成文本，模型看不到真图。
- 输入表示三套并存（ACP ContentBlock / codex UserInput / ContentPart）+ ≥4 个平行 flattener。

## Requirements

### R1 接缝类型 + 对齐 Codex 输入策略
- 在 protocol crate 定义 `UserInputBlock`（codex v2 `UserInput` 的别名/newtype），作为全项目 canonical 用户输入单元。调用方一律用此名，不直接 `use codex_app_server_protocol::UserInput`，为后续转自定义扩展留接缝。
- **收口到 Codex 实际策略**（pin: rust-v0.133.0 / 9474e5c）：message 与 steer 统一收 `Vec<UserInputBlock>`（Codex 的 turn/start 与 turn/steer 本就同形）。前端产出结构化变体，**弃 `<file:path>` 文本标记 hack**：`@`→`Mention`、slash/Skill→`Skill`、图片→`Image{data URL}`（磁盘来源可 `LocalImage`）、可视标记→`Text.text_elements`（byte-range，不污染字面文本）。详见 design.md §0。

### R2 唯一连接器映射
- 在连接器边界实现**唯一**的 `UserInputBlock → Vec<ContentPart>` 映射：`Image → ContentPart::Image{mime_type,data}`、`Text → ContentPart::Text`、`LocalImage/Skill/Mention → 明确定义（结构化或文本，给出依据）`。
- 替换 pi_agent `prompt`（`to_fallback_text`）、`steer_session`（`codex_user_input_to_text`）、`continuation`（`codex_user_input_to_message_part`）三处各自的拍平逻辑，统一走该映射。

### R3 API 入参统一
- message API 入参从 `prompt_blocks: Vec<JsonValue>(ContentBlock)` 改为 `input: Vec<UserInputBlock>`，与 steer 同形。
- `PromptPayload` 去 `Blocks(Vec<ContentBlock>)`，改承载 `Vec<UserInputBlock>`（或等价单形态）。
- 重新生成 TS 契约；保证既有前端发送路径（即便尚未重构）在新契约下可编译、可发送（text 路径不回归）。

### R4 删冗余 / 收敛 ContentBlock
- 删除或合并：`codex_user_input_to_text`、`content_block_to_text`、`codex_user_input_to_message_part`、`content_block_to_codex_user_input`/`content_blocks_to_codex_user_input` 中的重复职责。
- ContentBlock 仅保留在 ACP relay 边界（`relay_connector.rs` / compat），转换集中一处单实现；text 投影至多保留一个且仅用于标题/trace 摘要（注释标明用途）。
- 评估 compat/mod.rs 的 ContentBlock↔codex round-trip 是否可随其 P0.4 TODO 退场（不强制本 child 完成，但要给结论）。

## Acceptance Criteria

- [ ] 全项目用户输入调用方依赖 `UserInputBlock`，不直接耦合外部 codex 类型（接缝可后续替换为自定义类型）。
- [ ] pi_agent `prompt`/`steer`/`continuation` 均不再把图片拍平；图片经唯一映射成为 `ContentPart::Image`。
- [ ] message 与 steer API 入参同形（`Vec<UserInputBlock>`）；`PromptPayload` 不再持有 `ContentBlock`。
- [ ] 冗余 flattener 已删除/合并；ContentBlock 仅存于 relay 边界单处转换。
- [ ] 单测覆盖：`UserInputBlock → ContentPart` 映射（含 image）、API 反序列化、relay 边界往返。
- [ ] 后端相关 crate `cargo build` + 相关 `cargo test` 通过；TS 重新生成且 `pnpm -F app-web typecheck` 通过（前端发送路径适配最小改动）。

## 范围外

- 前端 composer 重构 / 图片采集 UI（child-2/3）。
- 双模 action 模型（child-4）。
- 自定义 UserInput 扩展类型的具体字段设计（本 child 只留接缝，不实现扩展）。

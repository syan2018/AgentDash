# [child-1] 执行计划

校验命令：
- 后端：`cargo build -p agentdash-agent-protocol -p agentdash-spi -p agentdash-application -p agentdash-executor -p agentdash-api`、相关 `cargo test`
- 契约：重新生成 TS（项目既有生成命令，见 `agentdash-contracts/generate_ts`）
- 前端：`pnpm -F app-web typecheck`

## 步骤（每步一组、可独立回滚）

### S1 接缝类型 + ContentPart::image（纯增量）
- [ ] protocol：`pub type UserInputBlock = codex::UserInput;` + re-export；agent-types：`ContentPart::image(mime,data)`。
- [ ] 编译通过（未改调用方）。

### S2 唯一映射 + 单测（纯增量）
- [ ] 实现 `user_input_blocks_to_content_parts`（含 data URL 解析）；单测覆盖 text/image/mention。
- 校验门：映射单测全绿。

### S3 连接器切换到映射
- [ ] pi_agent `steer_session`：`codex_user_input_to_text` → 统一映射 → `AgentMessage`。
- [ ] pi_agent `prompt`：`to_fallback_text` 投递路径 → 统一映射（保留 `to_fallback_text` 仅作摘要）。
- [ ] continuation：删 `codex_user_input_to_message_part`，改统一映射。
- 校验门：pi_agent 回归测试（带 image 的 input 产出 image part）。

### S4 PromptPayload + UserPromptInput 改形
- [ ] `PromptPayload` 去 `Blocks(ContentBlock)` → 承载 `Vec<UserInputBlock>`。
- [ ] `UserPromptInput.prompt_blocks` → `input`；`resolve_prompt_payload` 适配；`commit.rs` 去 ContentBlock 转换。
- [ ] 更新 hub/tests.rs、connector tests 断言。
- 校验门：application + spi `cargo test`。

### S5 API 入参统一 + TS 重生成
- [ ] message route + `LifecycleAgentMessageCommand/Delivery`：`prompt_blocks` → `input: Vec<UserInputBlock>`。
- [ ] 重新生成 TS 契约。
- [ ] 前端最小适配：`sendLifecycleAgentMessageByRuntimeSession` 请求体 `prompt_blocks` → `input`；SessionPage 现有文本发送改造到新字段（完整前端在 child-2/3）。
- 校验门：`pnpm -F app-web typecheck` + 现有文本发送手测可用。

### S6 ContentBlock 收敛 + relay 边界
- [ ] 删 `codex_user_input_to_text`、`content_block_to_text`（或并入摘要）、冗余 `content_block(s)_to_codex_user_input` 调用。
- [ ] relay_connector.rs / compat 的 ContentBlock 转换集中单实现；relay 往返单测。
- [ ] 给出 compat round-trip 是否随 P0.4 退场的结论（写进 closeout 或 spec）。
- 校验门：全相关 crate `cargo test` + relay 往返单测。

### S7 收尾
- [ ] trellis-check；spec 更新（canonical 输入路径写进 .trellis/spec backend session / cross-layer）。

## 回滚点
- S1/S2 纯增量，安全。
- S3 单连接器，回退即恢复旧 flatten。
- S4/S5 为契约改形，建议同一分支内分别原子提交；失败回退该提交。
- S6 删除项最后做，前序绿了再清理。

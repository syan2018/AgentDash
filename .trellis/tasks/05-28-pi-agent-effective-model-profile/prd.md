# 收束 PI_AGENT LLM Provider effective model profile 管线

## Goal

建立后端唯一事实源，输出当前身份下 PI_AGENT 实际可执行的 LLM Provider、模型列表、调用 profile 与不可用原因，并让 runtime prompt、discovered-options、前端模型选择共同消费它。

本任务要消除 `/llm-providers/effective`、PI_AGENT discovered-options、runtime bridge resolve、设置页 probe 四条路径各自拼装 Provider / 模型 / 凭据状态导致的不一致。

## Background

用户反馈当前没有个人 BYOK 时，项目实际 Key 提供模式不同也会被错误提示为“当前身份没有可用的 LLM Provider `athen-ai` 凭据，请在个人 BYOK 设置中补齐”。进一步 review 发现，上一轮提交修复了部分错误提示，但把 PI_AGENT 前端模型选择改成从 `/llm-providers/effective` 派生后，又会吃掉仅平台 Key Provider 的动态发现模型。

根因不是单个文案或下拉框 bug，而是模型可用性事实散落：

- `/llm-providers/effective` 只解析 Provider 凭据可用性，并返回 raw provider 配置。
- `PiAgentConnector::discover_options_stream_with_context` 另行按当前身份构建 provider entries 并动态加载模型。
- `PiAgentConnector::resolve_bridge_for_execution` 再次用 provider entries 解析 runtime bridge。
- 设置页 `probeModels` / `probeUserProviderModels` 使用独立探测路径。
- 前端 `useEffectiveLlmModelSelector` 在 UI 侧尝试把 effective Provider 与 model selector 拼起来，形成新的事实源。

## Confirmed Facts

- `credential_mode` 运行态语义已经在 `.trellis/spec/backend/capability/llm-model-config.md` 中定义：`global_only` 只用全局 DB/env Key，`global_or_user` 优先用户 BYOK 否则全局 Key，`user_required` 只用当前用户 BYOK。
- `discovered-options` 已通过 `DiscoveryContext.identity` 传入当前用户身份。
- HTTP prompt 已通过 `ExecutionSessionFrame.identity` 携带身份。
- `ProviderEntry::load_models_raw` 当前合并动态发现模型、配置模型、default_model，并应用 blocked 标记。
- `resolve_bridge_for_execution` 当前命中 provider_id 时会直接创建 bridge，没有拒绝 blocked / 未知模型。
- `EffectiveLlmProviderDto` 当前不包含动态模型、resolved wire api、模型 profile、discovery 状态或不可用原因。
- 当前工作区存在一个未提交的半成品前端补丁 `useEffectiveLlmModelSelector`，它仍然是 UI 侧拼事实，应在实现时删除或替换。

## Requirements

- 后端必须提供一个可复用的 effective model profile resolver，按当前身份一次性解析：
  - Provider 是否 enabled / executable。
  - 凭据来源：`global_db` / `global_env` / `user_byok` / `none`。
  - 不可用原因：禁用、缺全局凭据、缺用户 BYOK、无身份、凭据解密失败、wire_api 无效、models / blocked_models 配置无效、模型 discovery 失败等。
  - 实际调用 profile：provider slug、protocol、resolved wire api、base_url、discovery_url、credential source、模型 ID。
  - 实际可选模型 profile：id、name、provider_id、reasoning、supports_image、context_window、blocked、source、discovery status。
- PI_AGENT discovered-options 必须消费该 resolver 的结果，不再自己构建模型事实。
- PI_AGENT runtime bridge resolution 必须消费同一 resolver 的结果，并在后端拒绝不可执行 Provider、未知模型、blocked 模型、不可用凭据和无效 profile。
- 前端 ExecutorSelector 和 Project Agent preset editor 必须只消费后端返回的 model selector/profile，不再调用 `/llm-providers/effective` 自行合并模型。
- 设置页 Provider 管理可以保留临时 probe 能力，但该 probe 只服务编辑表单，不作为用户实际可用模型列表事实源。
- `global_only` 平台 Key Provider 的模型必须在当前用户无 BYOK 时仍可见且可调用。
- `user_required` Provider 在当前用户无 BYOK 时不得出现在可调用模型列表中，runtime 也必须拒绝旧配置或手动输入绕过。
- OpenAI-compatible 无 Key 本地端点仍按现有契约可执行，但必须在 profile 中显式标记 `credential_source = none`。
- `wire_api` 空值必须在后端 resolver 中解析成实际 `responses` 或 `completions`，前端不得推断。

## Acceptance Criteria

- [ ] 后端新增或重构出单一 `effective model profile` resolver，PI_AGENT discovery 和 runtime resolve 都从它读取 Provider / model / call profile。
- [ ] `/agents/discovered-options/stream?executor=PI_AGENT` 返回的模型列表与 runtime 实际可调用模型一致。
- [ ] 前端删除或停止使用 `useEffectiveLlmModelSelector` 的 `/effective + discovery` UI 侧拼装路径。
- [ ] `global_only` + 平台全局 Key 的 Provider 在没有个人 BYOK 时仍显示模型并可发起 prompt。
- [ ] `user_required` + 缺用户 BYOK 的 Provider 不显示模型；旧 preset / 手动模型输入触发 prompt 时返回准确错误。
- [ ] blocked 模型不应可选；即使旧配置或手动输入 blocked 模型，runtime 也返回准确错误。
- [ ] resolved wire api、credential source、模型 source/discovery 状态由后端 profile 给出。
- [ ] 设置页 probe 仍可用于临时探测，但不影响用户实际可用模型列表事实源。
- [ ] 后端测试覆盖 `global_only`、`global_or_user`、`user_required`、blocked、unknown model、dynamic discovery fallback/failed status。
- [ ] 前端类型检查通过，并覆盖模型 selector 使用后端 profile 的关键转换或组件逻辑。

## Out Of Scope

- 不引入向后兼容旧字段的长期双轨逻辑；预研项目应以新的正确事实源为准。
- 不改变数据库字段语义，除非实现 profile 缓存必须新增迁移；默认先做请求期解析，不做持久化缓存。
- 不重做 BYOK 设置页信息架构；设置页只调整与 effective model profile 事实源冲突的部分。
- 不新增 temperature / max_tokens / top_p 等业务配置。

## Open Questions

无阻塞问题。现有产品意图已经明确：后端维护完整状态获取与校验管线，收束散落路径，前端不做半吊子兜底。

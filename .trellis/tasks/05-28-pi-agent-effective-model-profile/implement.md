# Implementation Plan

## Preconditions

- 当前任务处于 `planning`，实现前需要 review 本 PRD/design/implement，并通过 `task.py start` 激活。
- 当前 worktree 存在与本任务无关的未提交文件：
  - `crates/agentdash-infrastructure/src/persistence/sqlite/session_repository.rs`
  - `crates/agentdash-infrastructure/migrations/0067_backfill_session_event_protocol_payloads.sql`
- 当前 worktree 还存在一个本任务相关但不应继续沿用的半成品修改：
  - `packages/app-web/src/features/executor-selector/model/useEffectiveLlmModelSelector.ts`

实现时必须只处理本任务相关文件，避免把无关 SQLite/migration 改动纳入提交。

## Ordered Checklist

### 1. 清理前端半成品路径

- 删除或替换 `useEffectiveLlmModelSelector` 的 `/llm-providers/effective + discovery` 合并逻辑。
- 恢复 `SessionChatView` 和 `PresetFormFields` 对 `discovered.options.model_selector` 的直接消费，直到后端 profile 接上。
- 保留 `ExecutorSelector` 中按 visible unblocked model count 控制空态的修复，如果它仍适配新 profile。

### 2. 后端新增 profile resolver

- 在 PI_AGENT provider registry 附近新增 `EffectiveLlmProfileResolver` 或等价函数。
- 将上一轮 `ProviderUnavailableReason` 迁入 resolver 的统一 unavailable reason。
- 提取可复用逻辑：
  - effective credential resolution
  - resolved OpenAI wire api
  - configured model parsing
  - blocked model parsing
  - dynamic discovery + configured/default 合并
- 产出 provider profile、model profile、call profile、discovery status。

### 3. PI_AGENT discovered-options 改用 profile resolver

- `discover_options_stream_with_context` 调用 profile resolver。
- `model_selector.providers` 只包含 executable provider。
- `model_selector.models` 只包含 executable provider 下的模型 profile，并携带 blocked 标记。
- `default_model` 来自第一个 executable provider 的有效 default profile。
- slash commands / permissions / agents 仍保留原来源。

### 4. PI_AGENT runtime resolve 改用 profile resolver

- `prompt` 每轮按 `ExecutionContext.session.identity` 加载 profile catalog。
- `resolve_bridge_for_execution` 基于 call profile 和 model profile 校验 provider/model。
- 拒绝：
  - provider 不存在 / disabled / unavailable
  - 缺全局凭据 / 缺用户 BYOK / 无身份读取 BYOK
  - unknown model
  - blocked model
  - provider_id 为空且 model_id 多 provider 命中
- bridge 构建使用 profile 中 resolved wire api/base_url/credential source 对应的 secret，避免再重新推断。

### 5. Contract / API 对齐

- 若 discovered-options 继续使用 ad-hoc JSON patch，可先不新增独立 endpoint，但 profile DTO 应尽量进入 `agentdash-contracts::llm_provider`，供后续 API 与前端复用。
- 如果新增 `/llm-providers/effective-model-profiles`，同步生成 TS contracts，并让前端通过 generated DTO 消费。
- 不扩展 `EffectiveLlmProviderDto` 为模型事实源；它可继续服务 BYOK 设置页。

### 6. 前端收束

- `SessionChatView` 和 `PresetFormFields` 只读后端 profile/discovered-options 输出。
- 删除 UI 侧解析 provider.models / blocked_models 的可调用模型逻辑。
- 设置页 provider row 可继续显示管理用 discovered/probed 模型，但不要影响 selector。

### 7. Tests

后端测试：

- `global_only` + global DB/env key：无用户 BYOK 时 provider executable，模型出现在 discovered-options，runtime 可调用。
- `global_or_user`：用户 BYOK 优先；无 BYOK 时回全局 Key。
- `user_required`：无用户身份或无用户 BYOK 时不可执行，模型不出现在 selector，runtime 旧配置被拒绝。
- OpenAI-compatible no-key local endpoint：`credential_source=none` 且可执行。
- invalid wire_api：provider unavailable，并返回准确 reason。
- blocked model：selector 标记/隐藏，runtime 拒绝。
- unknown model：runtime 拒绝。
- duplicate model without provider_id：runtime 要求 provider_id。
- dynamic discovery failure：profile 暴露 failed status，并按设计保留 configured/default 模型或返回无动态模型。

前端测试 / 检查：

- `pnpm --filter app-web run typecheck`
- 如存在轻量 mapper/selector helper，补单测验证前端不再解析 `/llm-providers/effective` raw provider config。

## Validation Commands

```powershell
cargo test -p agentdash-executor prompt_selected -- --nocapture
cargo test -p agentdash-executor effective
cargo test -p agentdash-contracts
pnpm run contracts:check
pnpm --filter app-web run typecheck
pnpm --filter app-web lint
```

根据实际改动范围补充更窄或更宽的 cargo test。

## Risk Points

- 不要在前端重新实现 blocked/default/configured model 合并。
- 不要让 `/llm-providers/effective` 成为模型 selector 的事实源。
- 不要让 runtime 继续接受 selector 之外的 blocked/unknown model。
- 不要泄漏明文 secret 到 DTO 或日志。
- 不要把设置页临时 probe 的结果写成“当前用户实际可用模型”。

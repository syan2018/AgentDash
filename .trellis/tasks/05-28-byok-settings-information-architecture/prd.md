# BYOK 与设置页信息架构重构

## Goal

支持 LLM Provider 的混合凭据模式：管理员可以维护少量组织级 Provider 与全局 Key，普通用户可以在被允许的 Provider 上配置自己的 BYOK 凭据，并按个人偏好选择默认 Provider / Model。设置页同步重构为清晰的信息架构，让平台级配置、个人凭据与偏好、项目设置、本机运行时管理各自落在稳定边界内。

## User Value

- 管理员可以为团队提供开箱即用的少量全局模型能力，同时把高成本或个人偏好的模型交给用户 BYOK。
- 用户不用接触系统级设置，也能维护自己的模型 Key 和默认模型偏好。
- 共享 Project / Agent Preset 继续引用稳定的 `provider_id` / `model_id`，真正发起调用时按当前用户身份解析凭据。
- 设置页从“一个大表单页面”收敛为可维护的配置中心，后续新增配置入口时不再扩大单页耦合。

## Confirmed Facts

- 当前 `llm_providers` 是全局 PostgreSQL 表，包含 provider metadata、`api_key`、`env_api_key`、模型列表与屏蔽列表；`/llm-providers` 现有接口只有 personal 模式或管理员可访问。
- 当前 `settings` 已有 `system` / `user` / `project` scope，后端会按 `CurrentUser` 解析 user scope，但前端 user 面板目前只展示浏览器本地 DebugPrefs。
- PiAgent 每轮 prompt 会从 `LlmProviderRepository` 动态重载 Provider，因此 Provider 变更无需重启后端才能进入运行态。
- `ExecutionSessionFrame.identity` 已存在，HTTP prompt 会把当前 `AuthIdentity` 注入 session construction，运行时可以按当前用户解析 BYOK 凭据。
- `discover_options_stream` 当前没有 `CurrentUser`，`AgentConnector::discover_options_stream` 也没有身份参数；模型发现若要按 BYOK 过滤，需要补齐 discovery 身份传递。
- `SettingsPage.tsx` 当前承载 Scope Tabs、Backend 管理、LLM Provider 管理、模型管理、Pi Agent 偏好、默认 Executor、Project 跳转、本机运行时入口，文件超过 2000 行，且存在局部 UI atom、inline SVG 和 provider/model 逻辑混杂。
- 现有 LLM Provider DTO 在 API route 与前端 `api/llmProviders.ts` 手写维护；跨层规范要求新增/修改前端消费的业务 DTO 优先进入 `agentdash-contracts` 并生成 TS。

## Requirements

### R1. 管理员维护全局 Provider Catalog

- 管理员可以继续创建、编辑、删除、排序全局 LLM Provider。
- Provider metadata 至少覆盖现有字段：`name`、`slug`、`protocol`、`base_url`、`wire_api`、`default_model`、`models`、`blocked_models`、`env_api_key`、`discovery_url`、`enabled`。
- Provider 增加凭据策略字段，表达该 Provider 是否使用全局 Key、是否允许用户 BYOK、是否必须由用户 BYOK。
- 管理员保存的 DB-backed 全局 Key 必须按密文保存；响应只返回配置状态和脱敏展示值。
- `env_api_key` 继续作为管理员定义的全局运行期凭据来源，归入全局凭据解析链路。

### R2. 用户维护自己的 BYOK 凭据和默认模型偏好

- 普通用户可以查看当前可 BYOK 的 Provider Catalog，但不能修改全局 Provider metadata、全局 Key、排序或启用状态。
- 用户可以为允许 BYOK 的 Provider 保存、替换、删除自己的 API Key。
- 用户 API Key 按 `user_id + provider_id` 隔离存储，任何列表/读取响应都不得返回原文。
- 用户可以在 user scope 保存默认 `executor`、`provider_id`、`model_id`、`thinking_level` 偏好，用于新会话没有显式配置时的个人默认值。
- 用户 BYOK 页面需要清楚区分：已由平台提供、已配置个人 Key、需要个人 Key 后可用、被管理员禁用。

### R3. Provider 凭据解析必须以当前身份为准

- PiAgent prompt、模型 discovery、模型 probe 使用同一套有效 Provider 解析规则。
- 解析规则按 Provider 凭据策略执行：
  - `global_only`：只使用管理员全局 DB Key 或 `env_api_key`。
  - `global_or_user`：当前用户配置了个人 Key 时优先使用个人 Key，否则使用全局 Key。
  - `user_required`：只使用当前用户个人 Key；没有个人 Key 时该 Provider 不进入可执行模型列表。
- 无用户身份的系统级执行只使用全局凭据；引用 `user_required` Provider 时返回可诊断错误。
- Project / Agent Preset 继续保存稳定的 `provider_id` / `model_id`，不保存用户私有 credential id。
- 当某个 Provider 因缺少当前用户可用凭据而不可执行时，前端模型选择和后端错误都需要提示用户去个人 BYOK 设置补齐。

### R4. 权限与 API 边界清晰

- 全局 Provider 管理接口保持管理员 / personal 模式可写。
- 用户 BYOK 接口对所有已认证用户开放，但只能操作当前用户自己的凭据。
- 用户 probe 只允许在 BYOK 策略允许的 Provider 上使用提交的临时 Key 或自己的已保存 Key；普通用户不能借 probe 读取或滥用全局 Key。
- 所有新增 wire DTO 使用 `snake_case`，并通过 `agentdash-contracts` 生成前端类型。

### R5. 设置页信息架构重构

- `/settings` 页面保留为配置中心，但页面本体只负责布局、导航、身份/Project 状态和面板装配。
- 设置页拆分为四个顶层面板：
  - `平台配置`：管理员可见，包含 Backend 管理、全局 LLM Provider Catalog、Pi Agent 系统级配置、系统默认 Executor。
  - `个人设置`：所有用户可见，包含 BYOK 凭据、个人默认模型偏好、DebugPrefs。
  - `项目设置`：保留当前 Project 设置入口和状态，不在全局设置页内重复项目配置表单。
  - `本机运行时`：desktop-only，继续挂载现有 `LocalRuntimeView`。
- LLM Provider 与模型管理逻辑从 `SettingsPage.tsx` 提取到 feature 模块；新增/复用 UI primitive 时遵守当前 design language。
- UI 需要避免卡片嵌套卡片，把重复 provider row、credential form、model chip/editor 组件拆成可测试的小组件。

### R6. 数据迁移和密钥治理

- PostgreSQL schema 通过新增递增 migration 表达，不修改历史 migration。
- BYOK 相关 DB-backed Key 使用统一密文列与密钥状态字段；旧全局明文 `api_key` 列迁移为新的密钥模型后不再作为运行态读取来源。
- Repository readiness 只检查 schema，不在启动路径补 DDL。
- 日志、错误和 tracing 不输出 API Key 原文、密文或 OAuth credential JSON。

## Scope Boundaries

- 本任务不新增新的 provider protocol adapter；沿用当前 `anthropic`、`gemini`、`openai_compatible`、`openai_codex` 能力。
- 本任务不做团队共享的“用户 Key 池”或 Project 级 credential delegation；用户私有 Key 只服务当前用户发起的执行。
- 本任务不扩展 OpenAI Codex OAuth 的多人账号授权流程，只把现有 Codex provider 纳入全局 / 用户凭据策略的 UI 与解析框架。
- 本任务不重写 ProjectSettingsPage；全局设置页只保留 Project 设置入口。

## Acceptance Criteria

- [ ] 管理员可以在设置页维护全局 Provider metadata、凭据策略、全局 Key 状态，普通用户无法访问全局写入口。
- [ ] 普通用户可以在个人设置中为允许 BYOK 的 Provider 保存、替换、删除自己的 Key，列表响应只显示脱敏值和配置状态。
- [ ] `global_only`、`global_or_user`、`user_required` 三种策略在 prompt、discovery、probe 中表现一致，并有后端测试覆盖。
- [ ] 当前用户配置个人 Key 后，ExecutorSelector / Agent Preset model selector 能发现该用户可用的 Provider / Model；删除个人 Key 后对应 `user_required` Provider 从可执行模型列表消失。
- [ ] 共享 Project / Agent Preset 引用同一个 `provider_id` 时，不同用户按自己的凭据解析；缺少个人 Key 的用户得到明确错误。
- [ ] 新增/修改的 LLM Provider API DTO 由 Rust contract 生成 TypeScript，`pnpm run contracts:check` 通过。
- [ ] PostgreSQL migration 创建/调整 BYOK 所需表和列，repository integration 路径通过 migration runner 初始化。
- [ ] `SettingsPage.tsx` 收敛为设置壳组件，LLM Provider、个人 BYOK、Backend、本机运行时等逻辑拆入 feature/ui/model 文件。
- [ ] 设置页在管理员、普通用户、desktop-only local runtime 三种状态下展示正确面板；文本和控件在移动/桌面宽度不重叠。
- [ ] 密钥原文不会出现在 API 响应、错误、日志或前端 store 可序列化状态中。

## Open Questions

无阻塞问题。规划采用的产品决策是：Provider Catalog 由管理员统一维护，用户 BYOK 只补充个人凭据与个人默认模型偏好；后续如需用户自建任意 Provider endpoint，应单独评估 Project 共享语义和安全边界。

## Notes

- 相关历史任务：`03-06-settings-config-panel` 建立 settings / Provider 配置雏形，`03-23-extend-llm-providers` 引入多 Provider 发现链路，`03-24-fix-piagent-dynamic-provider-bootstrap` 修正 Provider 动态重载。

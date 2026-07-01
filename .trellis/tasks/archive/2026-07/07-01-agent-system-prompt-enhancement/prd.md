# 补全 Agent System Prompt 体系

## 背景

当前 `default_system_prompt.md` 过于单薄且定位为 "coding agent"，与 AgentDash 作为通用 Agent 平台的实际定位不符。对比业界参考实现（Claude Code、Codex、Pi-mono），在安全行为、工具策略、环境感知、输出纪律等维度存在明显缺口。同时子代理（Companion）启动路径缺少环境上下文 frame 和父级上下文继承。

## 目标

1. 重写默认 system prompt，去除 coding-only 定位，建立通用 agent 行为基座
2. 新增 Environment Context Frame，让模型感知运行时环境
3. 补全 Companion 子代理的上下文继承机制
4. 补全 Compaction summary 的 handoff 提示

## 需求

### R1: 重写 default_system_prompt.md

**现状**: 身份写死 "AI coding agent"，仅有 Core Principles / Working Style / Progress Updates / Communication 四个 section，无安全指令、无工具策略、无输出约束。

**目标结构**:
- Identity: 通用 AI agent 身份，不绑定具体场景
- Core Principles: 保留 accuracy/minimal-footprint/respect-conventions，措辞通用化
- Action Safety: 可逆性判断、破坏性操作确认、不猜测凭据/URL、不主动暴露敏感信息
- Tool Usage: 先读后写、偏好专用工具而非通用 shell、并行调用、验证工具结果
- Output Style: 简洁规则、结构化进度、不过度叙述、跟随用户语言、不加无意义 emoji
- Communication: 合并现有 Progress Updates + Communication，精简

**约束**:
- 总长度控制在 ~800 词以内（当前 ~250 词，参考 Claude Code ~2000 词 → 取中间）
- 不出现任何 coding/software engineering 专属措辞（这些由 AgentPresetConfig 或 AGENTS.md 按需注入）
- 保持中立通用，让各种 preset agent（coding、research、ops、chat）都能在此基座上叠加

### R2: 新增 Environment Context Frame

**现状**: 模型不知道当前日期、平台、OS、自身 model 名称、工作目录。

**目标**: 在 system prompt 通道中注入环境信息 frame，内容包含：
- 当前日期时间（UTC + 可选 local timezone）
- Platform / OS version
- Agent runtime version（可选）
- Model identifier（从 executor config 取）
- Working directory（从 VFS root 取）

**delivery**: `model_channel = System`, `phase = SessionPolicy`, `order = 15`（identity 之后、guidelines 之前）

**构建时机**: 与 identity/guidelines 相同——`include_connector_startup_context == true` 时构建。

### R3: Companion 子代理上下文继承

**现状**: `resolve_companion_parent_facts()` 中 `parent_context_bundle` 始终为 `None`。子代理仅靠 dispatch_prompt + 自身 ProjectAgent preset 获取上下文，丢失父级运行时 user_preferences、环境信息等。

**目标**:
- 子代理启动时自动获得一个 `inherited_context` frame（channel = Context, phase = Assignment）
- 内容：父级 user_preferences（简要）+ 父级环境事实摘要 + 父级当前任务上下文摘要（如有 assignment fragments）
- 不传完整 parent_context_bundle（过重），而是在 companion dispatch 阶段构建一个轻量 summary

**约束**:
- inherited_context 总 token 预算 ≤ 500 tokens
- 仅在 companion 模式下注入，fork session 不受影响

### R4: Compaction Summary Handoff 提示

**现状**: compaction summary frame 直接注入摘要内容，但无元信息提示模型这是历史 handoff。

**目标**: 在 compaction_summary frame 的 rendered_text 头部加一段固定提示：

```
以下是之前对话的压缩摘要，用于延续工作上下文。摘要中的路径、函数名等具体信息可能已过时，请在执行前验证。
```

## 验收标准

- [ ] AC1: 新 default_system_prompt.md 不含 "coding"、"software engineering"、"编程" 等词
- [ ] AC2: 新 prompt 结构包含 Identity / Core Principles / Action Safety / Tool Usage / Output Style / Communication 六个 section
- [ ] AC3: Environment frame 在 Context Inspector UI 中可见，内容含 date + platform + model
- [ ] AC4: Companion 启动后，Context Inspector 显示 inherited_context frame，内容含父级偏好
- [ ] AC5: Compaction summary frame 头部含 handoff 提示文本
- [ ] AC6: 现有 identity_context_frame 测试通过（rendered_text 格式不变）
- [ ] AC7: 新增 environment_context_frame 单元测试
- [ ] AC8: 项目 `cargo check` 通过

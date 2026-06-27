# 技术设计：guidelines 独立帧 + identity 单一真相源

## 1. 现状链路（事实基线）

```
discover_mount_files (mount_file_discovery.rs)
  └─ BUILTIN_GUIDELINE_RULES 扫描 mount 根 + 一级子目录 → DiscoveredMountFile
      └─ capability_projection::derive_session_guidelines → Vec<DiscoveredGuideline>
          └─ launch/plan.rs / preparation.rs 携带 discovered_guidelines
              └─ build_identity_context_frame(IdentityFrameInput{ user_preferences, discovered_guidelines, .. })
                  └─ identity 帧:
                       sections = [Identity{ effective_prompt = base(+agent) }]   ← 只有身份
                       rendered_text = "## Identity .. ## User Preferences .. ## Project Guidelines .."  ← 身份+偏好+指引
```

消费侧（两条连接器路径）：

- **pi_agent**：`extract_identity_prompt(context.turn.context_frames)` 找 `kind=="identity"` 的帧 → 作为 `set_system_prompt`。`last_identity_prompt` 缓存用于"身份不变就不重置 system prompt"。今天的补丁让它优先取 `rendered_text`。
- **codex_bridge**：`compose_prompt_text(user_text, context.turn.context_frames)` 把**所有帧**的 `rendered_text` 拼进 prompt 文本（含 identity）。

帧通道路由（`preparation.rs`）：
- identity 帧 `delivery_channel = "connector_context"`，在 `enqueue_context_frames_for_transform_context` 的排除名单里（`kind=="identity" || kind=="pending_action"` → 不作为 turn-notice 投递）。
- 其余帧走 turn-start notice 通道 → 作为 user/turn 上下文投递给有状态 agent。

关键约束：identity 帧只在 `include_connector_startup_context`（新建/连接器启动）时构建 → guidelines 本质是**会话级系统指引**，不是 per-turn。

## 2. 根因

identity 帧把"系统提示词"的**两种表示**存了两份且可漂移：
- 结构化 `sections[Identity].effective_prompt`（身份）
- 手写拼接的 `rendered_text`（身份 + 偏好 + 指引）

guidelines / user_preferences **只存在于 `rendered_text`**。任何读结构化 section 的消费者都会静默丢数据。fallback 翻转只是换一份赢，没消除双份。

## 3. 目标设计（方案 B：独立系统级 guidelines 帧）

### 3.1 核心决策

| 决策点 | 选择 | 理由 |
|---|---|---|
| guidelines 放哪 | 从 identity 帧剥离，独立成新帧 | 分离"我是谁"与"项目规则"；与用户已确认方向一致 |
| 新帧投递通道 | `connector_context` 系统通道（与 identity 同级） | guidelines 是会话级系统指引，当前就在 system prompt；保持语义不回归 |
| user_preferences | 一并迁入新帧 | 与 guidelines 同病同源（只在 rendered_text）；不迁则净化 identity 时会丢 |
| 偏好/指引 section 粒度 | **拆两段**：独立 `UserPreferences` + `ProjectGuidelines` section | 来源不同（用户 vs 项目）、渲染分组清晰；用户已确认 |
| 单一真相源 | 每个帧 `rendered_text` 由其结构化 section 经唯一渲染函数派生 | 杜绝手写第二份拷贝 |
| 连接器系统提示词 | `extract_identity_prompt` → `assemble_system_prompt`：身份帧 + guidelines 帧按序拼接 | 单一组装路径；删除 fallback 补丁 |

### 3.2 备选方案与取舍

- **方案 A（保留在 identity 帧，但单一真相源）**：让 identity 帧的结构化字段承载完整系统提示词（含偏好+指引），`rendered_text` 由其派生。改动最小、连接器零改。**否决**：仍把项目指引焊在"身份"上，未解决语义层级错配，与用户选择的"独立帧"相悖。
- **方案 C（guidelines 走 turn-start user 通道）**：作为 user 消息投递。**否决**：把当前的 system 级指引降级为 user 消息，是行为回归，且 system vs user 指令对模型行为有别。
- **结论**：采用方案 B。方案 A 的"单一真相源"技巧在 B 中对 identity 帧仍然适用（identity 帧净化后 `rendered_text` 仅由身份派生）。

### 3.3 数据流（目标）

```
discovered_guidelines + user_preferences
  └─ build_guidelines_context_frame(GuidelinesFrameInput{ user_preferences, discovered_guidelines })
       └─ guidelines 帧:
            kind = "system_guidelines"
            delivery_channel = "connector_context"
            message_role = "system"
            sections = [ UserPreferences{..}, ProjectGuidelines{ entries:[{path, content}] } ]   ← 结构化
            rendered_text = render_guidelines(sections)   ← 单一派生，"## User Preferences" / "## Project Guidelines"

build_identity_context_frame(IdentityFrameInput{ base, agent, mode })   ← 不再含 preferences/guidelines
  └─ identity 帧:
       sections = [Identity{ effective_prompt }]
       rendered_text = effective_prompt（原样，无 "## Identity" 脚手架）   ← 仅由身份派生
```

> 注：identity 帧 `rendered_text` 刻意保持「原样身份提示词」，不包裹 `## Identity`
> 标题。历史上 connector 直接用 `effective_prompt` 投递给模型，无 AGENTS.md/偏好
> 时系统提示词与改造前逐字节一致（零回归）；guidelines 帧自带 `## User Preferences`
> / `## Project Guidelines` 标题来界定新增内容，assemble 时以 `\n\n` 顺序拼接。

消费侧：

```
pi_agent:
  assemble_system_prompt(frames):
    parts = []
    if let Some(idf) = frames.find(kind=="identity"):      parts.push(idf.rendered_text)   // 已是单一派生
    if let Some(gf)  = frames.find(kind=="system_guidelines"): parts.push(gf.rendered_text)
    parts.join("\n\n")    // 空则 None
  → set_system_prompt(assemble_system_prompt(..))
  → last_identity_prompt 缓存改存"组合后的系统提示词"，变更检测覆盖 guidelines 变化

codex_bridge:
  compose_prompt_text 已渲染所有帧 rendered_text → 自动包含 guidelines 帧，无需改动（验证不重复/不回归）
```

帧通道路由：
- `preparation.rs::enqueue_context_frames_for_transform_context` 排除名单加上 `"system_guidelines"`，避免它又作为 turn-notice 重复投递。
- guidelines 帧与 identity 一同 push 进 `turn_context_frames`（仅 `include_connector_startup_context` 时）。

### 3.4 AGENTS.md 合并语义（R5）

在发现层（`mount_file_discovery` / `derive_session_guidelines`）确定：
- **稳定排序**：按 (mount_id, 路径深度, 路径字典序) 排序，保证可复现。
- **去重**：按规范化路径去重（同一文件多 mount 命中只留一次）。
- **逐级追加 + 明确顺序**：所有命中文件按上述顺序拼接，渲染保留来源标注（`### {path}`），靠后视为更具体/优先，由模型自行裁决冲突。
- 深度：现状仅扫描根 + 一级子目录。本期保持深度策略不变但**显式文档化该限制**（嵌套更深的 AGENTS.md 不发现），避免"看起来全覆盖"的错觉；如低成本可扩展再评估。

**明确不做"深层覆盖"（段级替换）**：自动用深层 AGENTS.md 的同名段替换浅层，前提是"指引相对某个正在被操作的文件逐级解析"。但当前发现机制是会话启动时把根+一级子目录的命中文件**平铺收集**，没有"当前工作文件"锚点，定义不出"谁更近"，因此该语义无真实触发 case。AGENTS.md 是自由 markdown，段级 merge 还需靠标题文本匹配，脆弱且偏离主流实现（Claude Code/Codex 均不做自动段级覆盖）。仅当未来发现机制改为"按编辑文件向上逐级解析"时才重新评估——属独立特性，不在本期。

## 4. 受影响文件（预估）

| 文件 | 改动 |
|---|---|
| `crates/agentdash-spi/src/hooks/mod.rs` | `ContextFrameSection` 新增 `ProjectGuidelines` / `UserPreferences`（或合并的 `SystemGuidelines`）变体 |
| `crates/agentdash-application/src/session/identity_context_frame.rs` | 移除 `user_preferences`/`discovered_guidelines`，`rendered_text` 仅由身份派生 |
| `crates/agentdash-application/src/session/guidelines_context_frame.rs`（新） | 新帧 payload：结构化 section + 单一 `render_guidelines` |
| `crates/agentdash-application/src/session/mod.rs` | 导出新帧构建函数 |
| `crates/agentdash-application/src/session/launch/preparation.rs` | 构建 guidelines 帧；排除名单加 `system_guidelines`；preferences 来源接到新帧 |
| `crates/agentdash-executor/src/connectors/pi_agent/connector.rs` | `extract_identity_prompt` → `assemble_system_prompt`（身份+指引）；缓存/变更检测调整；删除 fallback 补丁 |
| `crates/agentdash-application/src/context/mount_file_discovery.rs` / `capability_projection.rs` | 合并排序 + 去重 + 优先级 |
| 相关 `*_tests.rs` | 调整/新增单测 |

## 5. 兼容性与风险

- **持久化/序列化**：`ContextFrameSection` 是 `#[serde(tag="kind")]` 标签联合，新增变体向后兼容（旧数据无该变体）。历史持久化的 identity 帧仍含旧 rendered_text，replay 路径走 `rendered_text` 不受影响。
- **dedupe_context_frames**：确认新 kind 不会被错误去重（按 id/kind 去重逻辑需核对）。
- **title-gen / summarizer / audit / bridge-replay**：这些消费者按 scope/kind 取帧；新增系统通道帧需确认 scope 设置正确，不污染标题生成。
- **刷新优化（R7）**：缓存从 `last_identity_prompt` 改为"组合系统提示词"后，guidelines 变化才会触发重置——比现状更正确。需保证 guidelines 不变时不误触发重建。
- **codex_bridge 不重复**：guidelines 帧会被 `compose_prompt_text` 渲染；需确认它不会与其它通道重复（identity 已在其中，是既有行为）。

## 6. 验证策略

- 单元：identity 帧净化后 `rendered_text` 不含 guidelines/preferences；guidelines 帧 `rendered_text` 由 section 派生；`assemble_system_prompt` 合并正确；合并排序/去重/就近优先；preparation 排除名单生效。
- 行为：构造含 AGENTS.md 的 VFS → 走 projection/preparation → 断言 pi_agent system prompt 含身份+指引、turn 上下文不重复；删除 AGENTS.md → 不含。
- 回归：codex_bridge `compose_prompt_text` 仍含指引。
- `cargo build` + application/executor/spi `cargo test`。
- 回滚点：每个阶段独立可编译可测；连接器改动单独成步，便于回退到"仅 application 侧重构 + 临时双读"过渡态。

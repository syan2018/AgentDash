# Skill 发现与注入系统

> 状态：planning
> 参考：`references/pi-mono/packages/coding-agent/src/core/skills.ts`

## 背景

当前系统要使用文件型提示内容，必须在 Task/Story 配置里显式声明 `ContextSourceRef`。这对"项目级可复用 prompt 片段"的场景体验差——每次配置都要手动指定路径。

pi-coding-agent 的 skill 系统提供了更好的模式：
- 约定目录下放 `SKILL.md` 文件即自动被发现
- 系统 prompt 里只注入 skill 的名字+描述+路径（指针），不注入内容
- 模型根据任务相关性自主决定是否用 read tool 加载 skill 内容（懒加载）
- 用户也可以通过 `/skill:name` slash command 显式触发

## 设计

### 1. Skill 文件格式

```markdown
---
name: skill-name           # 必填，kebab-case，最多 64 字符
description: "一行描述"    # 必填，最多 1024 字符，注入到 available_skills 列表
disable-model-invocation: false  # 可选，true 时不注入 available_skills，只允许用户 /skill: 触发
---

# Skill 正文

具体指令内容...相对路径会以 skill 所在目录为基准解析。
```

### 2. 发现路径（优先级从高到低）

1. **项目级**：`{cwd}/.agents/skills/` 和 `{cwd}/skills/`（Codex convention 对齐）
2. **用户级**：`~/.agents/skills/`（全局 fallback）
3. **Address space 内**：遍历当前 session 的 address space mounts，在每个挂载根下同样扫描上述两个目录
4. **Plugin 提供**：通过 `resources_discover` 事件，plugin 可动态返回额外的 skill 路径

扫描规则：
- 目录下如有 `SKILL.md`，该目录为一个 skill 根，不再递归
- 否则递归扫描子目录查找 `SKILL.md`
- 遵守 `.gitignore` / `.ignore` 排除规则
- 同名 skill 冲突：first-wins（按发现优先级），记录 diagnostic warning

### 3. 注入策略：懒加载指针模式

在 `build_runtime_system_prompt()` 的末尾追加 `<available_skills>` 块：

```xml
The following skills provide specialized instructions for specific tasks.
Use the read tool to load a skill's file when the task matches its description.
When a skill file references a relative path, resolve it against the skill's base directory.

<available_skills>
  <skill>
    <name>code-review</name>
    <description>执行代码审查，检查安全、性能、可维护性</description>
    <location>/project/.agents/skills/code-review/SKILL.md</location>
  </skill>
</available_skills>
```

- `disable-model-invocation: true` 的 skill **不出现**在此列表
- 只有 read tool 可用时才注入此块（对应我们的 `fs_read` / `read_file` 工具）

### 4. 用户触发：`/skill:name` slash command

- 所有发现的 skill 自动注册为 `/skill:{name}` slash command
- 用户输入 `/skill:code-review` 时，将 skill 内容读出并包裹为消息注入对话：
  ```xml
  <skill name="code-review" location="/path/to/SKILL.md">
  References are relative to /path/to/.agents/skills/code-review.

  [skill 正文]
  </skill>
  ```
- 支持附加参数：`/skill:code-review src/main.rs` → skill 内容 + 参数拼接

### 5. `resources_discover` Plugin 事件

Plugin API 新增事件类型，plugin 可在 session 启动/reload 时动态提供额外资源路径：

```rust
// agentdash-plugin-api
pub trait PluginResourceProvider {
    fn on_resources_discover(&self, ctx: ResourceDiscoverContext) -> ResourceDiscoverResult;
}

pub struct ResourceDiscoverContext {
    pub cwd: String,
    pub reason: ResourceDiscoverReason,  // Startup | Reload
}

pub struct ResourceDiscoverResult {
    pub skill_paths: Vec<String>,  // 绝对路径
}
```

### 6. Skill 冲突检测

- 按发现路径优先级取 first-wins
- 冲突时记录 `SkillDiagnostic { name, winner_path, conflicting_path, source }`
- 可通过 session diagnostics API 查询

## 实施要点

- Skill 解析逻辑放在 `agentdash-injection` crate（新文件 `src/skill_loader.rs`）
- Address space skill 发现通过 `AgentSession` 构建时传入已解析的 mount 根路径
- `build_runtime_system_prompt()` 接收 `Vec<SkillRef>` 参数（name + description + location）
- slash command 注册接入现有 slash command 机制

## 不做

- Prompt Templates（参数化模板，pi-mono 有但我们暂不引入）
- Skill 内容的语法高亮渲染

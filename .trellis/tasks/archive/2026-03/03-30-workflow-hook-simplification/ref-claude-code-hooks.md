# Claude Code Hook 系统官方参考

> 来源：https://code.claude.com/docs/en/hooks
> 抓取时间：2026-03-30

---

## 架构总览

Hook 系统有 3 层嵌套：
1. **Hook Event** — 生命周期触发点（如 `PreToolUse`）
2. **Matcher Group** — 过滤条件（如只对 `Bash` 工具触发）
3. **Hook Handler** — 执行体（command / http / prompt / agent）

通信方式：**JSON stdin → 脚本决策 → JSON stdout**（command 类型），或 POST body → response body（http 类型）。

---

## 4 种 Handler 类型

| Type | 字段 | 作用 |
|------|------|------|
| `command` | `command` | Shell 脚本；读 stdin JSON，写 stdout JSON |
| `http` | `url` | POST 请求到端点；response body 是 JSON 输出 |
| `prompt` | `prompt` | 单轮 Claude 评估；返回 yes/no 决策 |
| `agent` | `prompt` | 带工具（Read, Grep, Glob）的 subagent 验证 |

### 通用 Handler 字段
```json
{
  "type": "command",
  "if": "Bash(git *)",
  "timeout": 600,
  "statusMessage": "Checking...",
  "once": true
}
```

### Command 特有字段
- `async`: 后台运行不阻塞
- `shell`: `"bash"`（默认）或 `"powershell"`

### HTTP 特有字段
- `headers`: key-value 对，支持 `$VAR` 环境变量插值
- `allowedEnvVars`: 环境变量白名单

---

## Exit Code 语义（Command Hook）

| Exit Code | 含义 |
|-----------|------|
| `0` | 成功；Claude Code 解析 stdout 的 JSON |
| `2` | 阻塞性错误；stderr 作为错误信息反馈给 Claude/用户 |
| 其他 | 非阻塞性错误；stderr 仅在 verbose 模式显示 |

**关键**：JSON 输出**仅在 exit 0 时被处理**。Exit 2 会忽略所有 JSON。

---

## 通用 Input 字段（所有事件）

```json
{
  "session_id": "abc123",
  "transcript_path": "/path/to/transcript.jsonl",
  "cwd": "/current/working/dir",
  "permission_mode": "default",
  "hook_event_name": "PreToolUse",
  "agent_id": "...",
  "agent_type": "Explore"
}
```

---

## 全部 25 种 Hook 事件类型

### 1. `SessionStart`
- **Matcher**: `startup` | `resume` | `clear` | `compact`
- **Input**: `source`, `model`, 可选 `agent_type`
- **阻塞**: 否（exit 2 仅向用户显示 stderr）
- **输出**: `additionalContext`（注入 Claude 上下文）；可写 `$CLAUDE_ENV_FILE` 设置环境变量

```json
{ "hookSpecificOutput": { "hookEventName": "SessionStart", "additionalContext": "..." } }
```

### 2. `UserPromptSubmit`
- **Matcher**: 无（始终触发）
- **Input**: `prompt`（提交的文本）
- **阻塞**: 是（exit 2 阻塞并擦除 prompt）
- **输出**: `decision: "block"` + `reason`，或 `additionalContext`

### 3. `PreToolUse`
- **Matcher**: 工具名（`Bash`, `Edit`, `Write`, `Read`, `Glob`, `Grep`, `Agent`, `WebFetch`, `WebSearch`, `AskUserQuestion`, `ExitPlanMode`, `mcp__<server>__<tool>`）
- **Input**: `tool_name`, `tool_input`（按工具不同）, `tool_use_id`
- **阻塞**: 是

**各工具 input schema:**

| 工具 | 关键字段 |
|------|---------|
| `Bash` | `command`, `description`, `timeout`, `run_in_background` |
| `Write` | `file_path`, `content` |
| `Edit` | `file_path`, `old_string`, `new_string`, `replace_all` |
| `Read` | `file_path`, `offset`, `limit` |
| `Glob` | `pattern`, `path` |
| `Grep` | `pattern`, `path`, `glob`, `output_mode`, `-i`, `multiline` |
| `WebFetch` | `url`, `prompt` |
| `WebSearch` | `query`, `allowed_domains`, `blocked_domains` |
| `Agent` | `prompt`, `description`, `subagent_type`, `model` |

**输出:**
```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "allow",    // "allow" | "deny" | "ask"
    "permissionDecisionReason": "...",
    "updatedInput": { "command": "modified command" },
    "additionalContext": "工具执行前注入的上下文"
  }
}
```

### 4. `PermissionRequest`
- **Matcher**: 工具名（同 PreToolUse）
- **Input**: `tool_name`, `tool_input`, `permission_suggestions[]`
- **触发条件**: 权限对话框*将要*显示时
- **阻塞**: 是

**输出:**
```json
{
  "hookSpecificOutput": {
    "hookEventName": "PermissionRequest",
    "decision": {
      "behavior": "allow",
      "updatedInput": { "command": "..." },
      "updatedPermissions": [/* permission update entries */],
      "message": "拒绝原因",
      "interrupt": true
    }
  }
}
```

**Permission Update Entry 类型:**

| `type` | 效果 |
|--------|------|
| `addRules` | 添加 allow/deny/ask 规则 |
| `replaceRules` | 替换给定 behavior 的所有规则 |
| `removeRules` | 移除匹配的规则 |
| `setMode` | 修改 permission mode |
| `addDirectories` | 添加工作目录 |
| `removeDirectories` | 移除工作目录 |

### 5. `PostToolUse`
- **Matcher**: 工具名
- **Input**: `tool_name`, `tool_input`, `tool_response`, `tool_use_id`
- **阻塞**: 否（工具已执行），但可以向 Claude 发送反馈

**输出:**
```json
{
  "decision": "block",
  "reason": "显示给 Claude",
  "hookSpecificOutput": {
    "hookEventName": "PostToolUse",
    "additionalContext": "...",
    "updatedMCPToolOutput": "..."   // 仅 MCP 工具
  }
}
```

### 6. `PostToolUseFailure`
- **Matcher**: 工具名
- **Input**: `tool_name`, `tool_input`, `tool_use_id`, `error`, `is_interrupt`
- **阻塞**: 否
- **输出**: `additionalContext`

### 7. `Stop`
- **Matcher**: 无（始终触发）
- **Input**: `stop_hook_active` (bool), `last_assistant_message`
- **阻塞**: 是（可阻止 Claude 停止）
- **关键**: 检查 `stop_hook_active` 以防止无限循环

### 8. `SubagentStart`
- **Matcher**: Agent 类型（`Bash`, `Explore`, `Plan`, 自定义）
- **Input**: `agent_id`, `agent_type`
- **阻塞**: 否
- **输出**: `additionalContext`（注入 subagent 上下文）

### 9. `SubagentStop`
- **Matcher**: Agent 类型
- **Input**: `stop_hook_active`, `agent_id`, `agent_type`, `agent_transcript_path`, `last_assistant_message`
- **阻塞**: 是（同 Stop）

### 10. `StopFailure`
- **Matcher**: `rate_limit` | `authentication_failed` | `billing_error` | `invalid_request` | `server_error` | `max_output_tokens` | `unknown`
- **Input**: `error`, `error_details`, `last_assistant_message`
- **阻塞**: 否（输出和 exit code 被忽略）
- **用途**: 仅日志/告警

### 11. `Notification`
- **Matcher**: `permission_prompt` | `idle_prompt` | `auth_success` | `elicitation_dialog`
- **Input**: `message`, `title`, `notification_type`
- **阻塞**: 否
- **输出**: `additionalContext`

### 12. `TeammateIdle`
- **Matcher**: 无
- **Input**: `teammate_name`, `team_name`
- **阻塞**: 是
- **控制**: Exit 2 → 反馈并继续；`{"continue": false, "stopReason": "..."}` → 完全停止

### 13. `TaskCreated`
- **Matcher**: 无
- **Input**: `task_id`, `task_subject`, `task_description`, `teammate_name`, `team_name`
- **阻塞**: 是

### 14. `TaskCompleted`
- **Matcher**: 无
- **Input**: `task_id`, `task_subject`, `task_description`, `teammate_name`, `team_name`
- **阻塞**: 是

### 15. `ConfigChange`
- **Matcher**: `user_settings` | `project_settings` | `local_settings` | `policy_settings` | `skills`
- **Input**: `source`, `file_path`
- **阻塞**: 是（`policy_settings` 除外）

### 16. `CwdChanged`
- **Matcher**: 无
- **阻塞**: 否
- **特殊**: 可通过 `$CLAUDE_ENV_FILE` 设置环境变量

### 17. `FileChanged`
- **Matcher**: 文件名 basename（如 `.envrc`, `.env`）
- **阻塞**: 否
- **特殊**: 可通过 `$CLAUDE_ENV_FILE` 设置环境变量

### 18. `WorktreeCreate`
- **Matcher**: 无
- **阻塞**: 是（任何非零 exit code 导致创建失败）
- **特殊**: Hook 提供 worktree 路径（stdout 打印路径）

### 19. `WorktreeRemove`
- **Matcher**: 无
- **阻塞**: 否

### 20. `PreCompact`
- **Matcher**: `manual` | `auto`
- **阻塞**: 否

### 21. `PostCompact`
- **Matcher**: `manual` | `auto`
- **阻塞**: 否

### 22. `Elicitation`
- **Matcher**: MCP server 名称
- **阻塞**: 是
- **输出**: `action: "accept" | "decline" | "cancel"` + `content`

### 23. `ElicitationResult`
- **Matcher**: MCP server 名称
- **阻塞**: 是（可覆盖用户响应）

### 24. `InstructionsLoaded`
- **Matcher**: `session_start` | `nested_traversal` | `path_glob_match` | `include` | `compact`
- **阻塞**: 否
- **用途**: 仅审计/可观测性

### 25. `SessionEnd`
- **Matcher**: `clear` | `resume` | `logout` | `prompt_input_exit` | `bypass_permissions_disabled` | `other`
- **阻塞**: 否

---

## 决策控制汇总

| 事件 | 阻塞方式 | 关键输出字段 |
|------|---------|-------------|
| `PreToolUse` | `permissionDecision: "deny"` | `permissionDecision`, `updatedInput`, `additionalContext` |
| `PermissionRequest` | `decision.behavior: "deny"` | `behavior`, `updatedInput`, `updatedPermissions` |
| `UserPromptSubmit`, `PostToolUse`, `Stop`, `SubagentStop`, `ConfigChange` | `decision: "block"` | `decision`, `reason` |
| `TeammateIdle`, `TaskCreated`, `TaskCompleted` | Exit 2 或 `continue: false` | stderr 或 `stopReason` |
| `WorktreeCreate` | 任何非零 exit | stdout 路径 |
| `Elicitation`, `ElicitationResult` | Exit 2 或 `action: "decline"` | `action`, `content` |
| 所有事件 | `continue: false` | `stopReason`（完全停止 Claude） |

---

## 通用 JSON 输出字段

```json
{
  "continue": false,          // 完全停止 Claude（所有事件）
  "stopReason": "message",    // continue=false 时显示给用户
  "suppressOutput": true,     // 对 verbose 模式隐藏 stdout
  "systemMessage": "warning"  // 向用户显示的警告
}
```

---

## 配置位置与作用域

| 位置 | 作用域 |
|------|--------|
| `~/.claude/settings.json` | 所有项目，不可共享 |
| `.claude/settings.json` | 单个项目，可提交 |
| `.claude/settings.local.json` | 单个项目，gitignored |
| Managed policy settings | 组织级 |
| Plugin `hooks/hooks.json` | 插件启用时 |
| Skill/agent frontmatter | 组件活跃时 |

**关键环境变量:**
- `$CLAUDE_PROJECT_DIR` — 项目根目录
- `${CLAUDE_PLUGIN_ROOT}` — 插件安装目录
- `${CLAUDE_PLUGIN_DATA}` — 插件持久化数据
- `$CLAUDE_ENV_FILE` — 环境变量持久化
- `$CLAUDE_CODE_REMOTE` — 远程环境标识

---

## 设计哲学总结

1. **机制简单，脚本复杂** — 引擎只做路由和 JSON 协议，复杂度推给外部脚本
2. **单一注入通道** — 所有上下文注入都走 `additionalContext: string`
3. **无溯源负担** — hook 脚本的输出就是最终结果，没有 source layer / priority / ref 追踪
4. **外部进程模型** — 任何语言、任何可执行文件，通过 stdin/stdout JSON 通信
5. **优雅降级** — hook 静默退出（code 0, 无输出）即为 no-op；错误不崩溃 session
6. **25 种事件，统一 API 表面** — 所有事件共享同一个 input/output JSON 框架

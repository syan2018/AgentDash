# VFS 资源本机物化契约

> 本文定义云端 VFS URI 如何被物化为本机可访问 path / workdir / URL。它补充
> [VFS Access](./vfs-access.md)：`vfs-access.md` 负责统一 mount/provider/runtime tool
> 边界；本文负责“资源落到本机后放哪里、如何复用、何时刷新”。

---

## 1. Scope / Trigger

当本机消费者不能直接读取 VFS URI 时，必须使用本文的物化规则：

- `shell_exec.command` 中出现 `mount://path`
- relay MCP tool arguments 中出现 `mount://path`
- 本机 MCP server / CLI / browser preview 需要本机 path 或短期 URL
- 目录参数需要本机稳定工作副本，例如 `uv init skill-assets://skills/foo`

物化不是 VFS 写回机制。Agent 仍以 VFS URI 作为稳定语义；本机 path 只是运行时桥接产物。

---

## 2. Core Decision

**默认公共稳定物化，只有资源语义明确绑定 session 时才进入 session scope。**

原因：

- `skill-assets`、内置 skill、项目级共享资料等内容大多可跨 session 复用。
- 本机用户需要可读、稳定、可复用的目录，而不是 `backend/session/uuid` 临时展开包。
- `plan_id / turn_id / tool_call_id` 是审计归因，不是资源身份。
- `lifecycle_vfs` 是 run / step / session 的动态投影，应跟随 session 清理和隔离。

---

## 3. 术语

### Materialization Root

一次物化的资源根。物化单位必须优先是语义 root，而不是单个命中的 URI：

- `skill-assets://skills/foo/scripts/check.sh` 的 root 是 `skill-assets://skills/foo`
- `skill-assets://skills/foo` 的 root 是它自己
- 普通文件 `inline://briefs/a.md` 的 root 可以是父目录或 provider 声明的 container root
- `lifecycle://...` 的 root 必须包含足够的 run / step / session 语义

### Materialization Scope

```rust
enum MaterializationScope {
    Public,
    Session { session_id: String },
}
```

- `Public`：同一台本机、同一用户、同一资源 root 可跨 session 复用。
- `Session`：资源内容依赖当前 session / lifecycle run / tool call 权限，只在该 session 下稳定。

### Access Mode

```rust
enum MaterializationAccessMode {
    ReadOnly,
    WritableWorkdir,
    Temporary,
}
```

- `ReadOnly`：脚本执行、文件读取、预览等只读场景。
- `WritableWorkdir`：目录参数可能被本机命令写入，例如 `uv init <dir>`、`npm install <dir>`。
- `Temporary`：短期 URL token、一次性中间文件、明确不应复用的 tool-call 产物。

### Materialization Key

稳定路径 key 由资源身份生成，不能由执行归因生成。

建议 seed 至少包含：

- `provider`
- `mount_id`
- `mount.root_ref`
- link resolution 后的 `root_uri`
- `scope` 类型和必要身份
- `access_mode`
- provider 声明的 source identity / version（如有）

不得使用以下字段决定稳定路径：

- `plan_id`
- `turn_id`
- `tool_call_id`
- `backend_id` / `local-dev-1`
- 单次 relay message id

这些字段只进入 `manifest.json` / audit trace。

---

## 4. 本机目录布局

默认根目录：

```text
{local_data}/agentdash/materialized/
```

其中 `{local_data}` 由本机 backend 决定，优先使用平台标准用户数据目录；不得默认使用 workspace 根目录。

### 4.1 公共只读物化

```text
{local_data}/agentdash/materialized/readonly/
  {provider}/{readable-root}--{short-key}/
    manifest.json
    content/...
```

示例：

```text
.../readonly/skill-assets/skills/foo--a1b2c3d4/
  manifest.json
  content/
    SKILL.md
    scripts/check.sh
    references/rules.md
    assets/logo.png
```

### 4.2 公共可写工作副本

```text
{local_data}/agentdash/materialized/workdirs/
  {provider}/{readable-root}--{short-key}/
    manifest.json
    content/...
```

示例：

```text
uv init skill-assets://skills/foo
```

rewrite 后应指向：

```text
.../workdirs/skill-assets/skills/foo--a1b2c3d4/content
```

公共 workdir 不随 session 清理。它必须通过 manifest 记录来源、source digest、dirty 状态和 last_used。

### 4.3 Session 级物化

```text
{local_data}/agentdash/materialized/sessions/{session_id}/
  readonly/{provider}/{readable-root}--{short-key}/
    manifest.json
    content/...
  workdirs/{provider}/{readable-root}--{short-key}/
    manifest.json
    content/...
  temp/{tool-call-or-token}/...
```

`lifecycle_vfs` 默认使用这一层。

### 4.4 用户可见路径规则

- 路径必须包含可读 `provider` 和 `readable-root`。
- `short-key` 只用于消歧，长度建议 8-12 hex。
- `backend_id`、`local-dev-1`、`plan_id`、`turn_id` 不得出现在默认用户可见路径中。
- Windows 与 Unix 路径分隔由本机 filesystem 决定；manifest 中的 entry path 一律使用 `/`。

---

## 5. Provider 默认映射

| Provider / URI | 默认 Scope | 默认 Access | Root 规则 | 说明 |
| --- | --- | --- | --- | --- |
| `skill_asset_fs` / `skill-assets://skills/{key}` | `Public` | 目录参数为 `WritableWorkdir`；脚本/文件为 `ReadOnly` | `skills/{key}` | skill root 是资源组单位，保留 `SKILL.md / scripts / references / assets` |
| `skill_asset_fs` / `skill-assets://skills/{key}/scripts/*` | `Public` | `ReadOnly` | `skills/{key}` | rewrite 指向 primary script，但本机目录包含整个 skill 资源组 |
| `inline_fs` | 默认 `Public` | 默认 `ReadOnly`；目录写入需显式请求 `WritableWorkdir` | provider 声明 container root 或 URI 父目录 | 项目/故事共享文本不应默认进入 session |
| `canvas_fs` | 默认 `Public` 或 project-scoped public | 默认 `ReadOnly`；编辑需显式能力 | canvas / asset root | 共享 canvas 资产不跟单个 session 绑定 |
| `relay_fs` 同 backend | 不物化 | 原始本机路径 | mount root + relative path | 直接 rewrite 为 workspace path，不复制 |
| `relay_fs` 跨 backend | 目标本机物化 | 按调用语义选择 `ReadOnly` / `WritableWorkdir` | workspace subtree 或文件父目录 | 只在跨机器访问时复制 |
| `lifecycle_vfs` | `Session { session_id }` | 默认 `ReadOnly`；目录参数可为 session `WritableWorkdir` | run / step / session 投影 root | 动态投影依赖当前 session，不能公共复用 |
| future plugin provider | provider 声明 | provider 声明 | provider 声明 | 未声明时保守使用 `Session + ReadOnly` |

### 5.1 `skill_asset_fs`

`skill_asset_fs` 是公共资源，默认不得放入 session 目录。

命中任一 skill 内资源时：

```text
skill-assets://skills/foo/scripts/check.sh
skill-assets://skills/foo/references/rules.md
skill-assets://skills/foo/assets/logo.png
```

均应规划为：

```text
root_uri = skill-assets://skills/foo
entries = SKILL.md + scripts/** + references/** + assets/**
scope = Public
```

如果 URI 指向目录 root：

```text
skill-assets://skills/foo
skill-assets://skills/foo/
```

在 shell/MCP 参数里默认按 `WritableWorkdir` 处理，因为本机命令可能写入该目录。

### 5.2 `lifecycle_vfs`

`lifecycle_vfs` 可以暴露 skill-like projection，但 scope 仍然是 session：

```text
lifecycle://skills/foo/scripts/check.sh
```

可以复用 skill resource-set 规划规则：

```text
root_uri = lifecycle://skills/foo
entries = SKILL.md + scripts/** + references/** + assets/**
scope = Session { session_id }
```

不得把 lifecycle projection 与 `skill_asset_fs` 的公共目录合并。它的内容来源、权限和生命周期依赖当前 run / step / session。

### 5.3 `relay_fs`

如果 source mount 和执行目标 backend 是同一台本机：

```text
main://src/lib.rs
```

应直接 rewrite 为：

```text
{mount.root_ref}/src/lib.rs
```

不创建 materialized cache。

如果 source mount 属于另一台 backend，必须通过物化传输到目标本机。此时 scope 取决于资源语义；若只是当前 session 临时跨机执行，可使用 `Session`。

---

## 6. Planner 输出契约

Application 层必须把所有 provider-specific 判断收敛为统一 policy，再交给 relay/local 执行：

```rust
struct MaterializationPolicy {
    scope: MaterializationScope,
    access_mode: MaterializationAccessMode,
    root_uri: String,
    readable_root: String,
    key_seed: MaterializationKeySeed,
    invalidation: InvalidationPolicy,
}

enum InvalidationPolicy {
    ExplicitVfsUpdate,
    SessionEnd,
    ManualRefresh,
}
```

provider-specific 逻辑只能出现在 planner / policy resolver 中；local store 不应理解 `skill-assets`、`lifecycle` 等业务 URI 语义，只根据 policy 写入正确路径。

---

## 7. Rewrite 规则

### 7.1 shell_exec.command

执行顺序：

1. 扫描命令中的 session mount URI。
2. 对每个 URI resolve link，得到真实 `ResourceRef`。
3. 根据 provider / path / stat / 调用语义生成 `MaterializationPolicy`。
4. 在单次 command 内按 materialization key 去重。
5. 物化成功后，用本机 path 替换 URI。
6. 按目标本机 shell flavor 做 quoting。

`cmd /C` 与 `sh -c` 的 quoting 规则不同，不得使用同一套双引号转义逻辑覆盖所有平台。

### 7.2 relay MCP arguments

- 扫描 JSON string leaf。
- 同一个 tool call 内按 materialization key 去重。
- path-like 字段默认 rewrite 为本机 path。
- URL-like 字段只有在显式 URL policy 下才转为短期 URL。
- 所有 relay MCP 调用入口必须携带 session/VFS/identity context；不能出现“session 内调用会 rewrite，RuntimeGateway 调用不 rewrite”的分叉。

### 7.3 URL 转换

URL 转换必须是显式 API / 显式 policy。不得对普通文本中的 VFS URI 自动转 URL。

短期 URL token 可以进入 session temp scope；token 到期或 session 结束后应失效。

---

## 8. Manifest 契约

每个 materialization root 必须包含 `manifest.json`，至少记录：

- `provider`
- `mount_id`
- `mount_root_ref`
- `scope`
- `access_mode`
- `source_uri`
- `resolved_root_uri`
- `readable_root`
- `materialization_key`
- `source_manifest_digest`
- `entries`：relative path、digest、size、mime、executable hint
- `created_at`
- `last_used_at`
- `dirty`
- `audit`：最近一次 `plan_id / session_id / turn_id / tool_call_id / backend_id`

`audit` 字段只用于追踪最近触发来源，不得反向决定路径。

---

## 9. 刷新与失效

默认不在每次工具调用时重新同步。

触发刷新 / 失效的来源：

- 云端明确发出 VFS 资源更新事件，携带 provider/root 或 materialization key。
- 用户或系统显式请求 refresh。
- manifest 缺失或无法解析。
- source digest 与 manifest 不一致，且本地不 dirty。
- 本地 workdir dirty 且 source digest 变化时，进入 conflict，不静默覆盖。

公共 readonly 可安全重建；公共 workdir 必须保护 dirty 内容；session 资源在 session 结束后可清理。

---

## 10. 错误语义

| 条件 | 预期 |
| --- | --- |
| URI mount 不存在 | 拒绝执行，返回 mount not found |
| source mount 无 `Read` capability | 拒绝物化 |
| 目录 URI 超出文件数/大小限制 | 拒绝物化 |
| entry path 含 `..`、绝对路径、空 segment | 拒绝写入 |
| digest / size 不匹配 | 拒绝写入 |
| public workdir dirty 且 source 更新 | conflict，等待显式处理 |
| lifecycle 缺少 session_id | 拒绝物化 |
| relay target backend 离线 | 拒绝工具调用，不做半成功 rewrite |

---

## 11. Tests Required

后续实现必须补齐以下测试：

- 两次物化同一 `skill-assets://skills/foo`，即使 `plan_id` 不同，也返回同一本机路径。
- `skill-assets` 公共路径不包含 `session_id`、`backend_id`、`plan_id`。
- `lifecycle_vfs` 物化路径必须包含 `sessions/{session_id}`。
- `skill-assets://skills/foo/scripts/check.sh` 物化包含 `SKILL.md / scripts/** / references/** / assets/**`，rewrite 指向 primary script。
- `uv init skill-assets://skills/foo` rewrite 到公共 `workdirs/.../content` 目录。
- 同一个 shell command / MCP JSON arguments 内重复引用同一 root 时只下发一次 materialize 请求。
- 同 backend `relay_fs` 直接 rewrite 到 workspace path，不创建 materialized cache。
- RuntimeGateway relay MCP 调用与 session 内 relay MCP 调用都能执行 VFS URI rewrite。
- Windows `cmd /C` 与 Unix `sh -c` 的 quoting 分别覆盖空格、中文、引号和 shell 特殊字符。
- 云端 VFS 更新事件能按 materialization key 使公共 readonly 失效；dirty workdir 遇到 source 变化进入 conflict。

---

## 12. Wrong vs Correct

### Wrong

```text
{temp}/agentdash/materialized/local-dev-1/{session_id}/{plan_id}/content/...
```

问题：

- `local-dev-1` 对本机用户没有路径语义。
- `session_id` 让公共 skill 无法跨 session 复用。
- `plan_id` 每次变化，真实 cache hit 失效。
- 路径不可读，无法判断目录对应哪个资源。

### Correct

```text
{local_data}/agentdash/materialized/readonly/skill-assets/skills/foo--a1b2c3d4/content/...
{local_data}/agentdash/materialized/workdirs/skill-assets/skills/foo--a1b2c3d4/content/...
{local_data}/agentdash/materialized/sessions/{session_id}/readonly/lifecycle/skills/foo--a1b2c3d4/content/...
```

这些路径同时满足：

- 人能读懂来源。
- 同一资源稳定复用。
- 公共资源不被 session 污染。
- session 动态投影仍有清晰隔离边界。

---

## 13. Non-Goals

- 不把 materialized path 作为 VFS 写入口。
- 不自动把 public workdir 写回云端；publish/import 必须是独立显式能力。
- 不做隐式实时同步；刷新由明确 VFS 更新事件或显式 refresh 触发。
- 不要求 local store 理解 provider 业务语义；业务语义由 application planner 输出 policy。

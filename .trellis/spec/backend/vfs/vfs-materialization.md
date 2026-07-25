# VFS 资源本机物化契约

> 云端 VFS URI 如何物化为本机可访问 path / workdir / URL。
> 补充 [vfs-access.md](./vfs-access.md)（统一 mount/provider/tool 边界）。

---

## 核心决策

**默认公共稳定物化，只有资源语义明确绑定 session 时才进入 session scope。**

- 公共资源（skill-assets、内置 skill、项目级共享资料）可跨 session 复用
- 物化路径必须可读、稳定，不含 `plan_id / turn_id / tool_call_id / backend_id`
- AgentRun workspace资源身份来自LifecycleRun/Agent/current AgentFrame与canonical Runtime binding；执行期node mount继续使用`orchestration_id + node_path + attempt`产品证据。物化cache scope以binding/thread与资源digest隔离。

---

## Materialization Key 规则

稳定路径 key 由**资源身份**生成：provider、mount_id、root_ref、root_uri、scope、access_mode。

**禁止用于 key**：`plan_id`、`turn_id`、`tool_call_id`、`backend_id`、relay message id。
这些字段只进入 manifest 的 `audit` 区域。

## 本机目录布局

```text
{local_data}/agentdash/materialized/
├── readonly/{provider-or-mount}/{readable-root}/     # 公共只读
├── workdirs/{provider-or-mount}/{readable-root}/     # 公共可写工作副本
└── sessions/{session_id}/                            # Session 级物化
    ├── readonly/...
    ├── workdirs/...
    └── temp/...
```

路径规则：
- 必须包含可读 `provider-or-mount` 和 `readable-root`
- 默认路径不附加 hash 后缀（冲突时才追加 `~{short-key}` 消歧）
- 不套 `content/` 包装层
- `backend_id`、`plan_id`、`turn_id` 不得出现在用户可见路径中

## Provider 默认映射

| Provider / URI | 默认 Scope | 默认 Access | 说明 |
| --- | --- | --- | --- |
| `skill_asset_fs` | `Public` | 脚本 `ReadOnly`，目录参数 `WritableWorkdir` | skill root 是资源组单位 |
| `inline_fs` | `Public` | `ReadOnly` | 项目/故事共享文本 |
| `canvas_fs` | `Public` | `ReadOnly` | 共享 canvas 资产 |
| `relay_fs`（同 backend） | 不物化 | 直接 rewrite 为 workspace path | 不复制 |
| `relay_fs`（跨 backend） | 按语义 | `ReadOnly` / `WritableWorkdir` | 只在跨机器时复制 |
| `lifecycle_vfs` | `Session` | `ReadOnly` | AgentRun delivery session 与 runtime node 动态投影；本地物化副本随 runtime session 收口 |

---

## Rewrite 规则

### shell_exec.command

1. 扫描命令中的 session mount URI
2. 对每个 URI resolve link，生成 MaterializationPolicy
3. 按 materialization key 去重
4. 物化后用本机 path 替换 URI
5. 按目标 shell flavor 做 quoting（`cmd /C` 与 `sh -c` 规则不同）

### relay MCP arguments

- 扫描 JSON string leaf，按 key 去重
- path-like 字段默认 rewrite 为本机 path
- 所有 relay MCP 调用入口必须携带 session/VFS context

---

## Manifest 契约

每个 materialization root 必须包含 `.agentdash-materialization.json`，记录：
provider、mount_id、scope、access_mode、source_uri、readable_root、materialization_key、source_manifest_digest、entries、dirty 状态、audit（最近触发来源）。

`.agentdash-materialization.json` 是保留文件名，VFS entry 不得覆盖。

## 刷新与失效

默认不在每次工具调用时重新同步。触发来源：
- 云端 VFS 资源更新事件
- 用户或系统显式 refresh
- manifest 缺失或 digest 不一致
- 本地 workdir dirty 且 source 变化 → conflict，不静默覆盖

## 错误语义

| 条件 | 预期 |
| --- | --- |
| URI mount 不存在 | 拒绝执行 |
| source mount 无 Read capability | 拒绝物化 |
| 目录超出大小限制 | 拒绝物化 |
| entry path 含 `..` / 绝对路径 | 拒绝写入 |
| public workdir dirty 且 source 更新 | conflict |
| relay target backend 离线 | 拒绝工具调用 |

## Non-Goals

- 不把 materialized path 作为 VFS 写入口
- 不自动把 public workdir 写回云端
- 不做隐式实时同步
- local store 不理解 provider 业务语义，只根据 policy 执行

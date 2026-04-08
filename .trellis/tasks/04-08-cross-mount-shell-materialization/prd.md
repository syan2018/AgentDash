# 跨 Mount URI 物化层

> 状态：planning
> 前置依赖：`04-08-tool-execution-improvements`（ShellExecInterceptor 框架）

## 问题背景

Skill 等资源可以寄存在云端 mount（只有 Read/List 能力，无 Exec），但工具实际执行时
（bash、本地 MCP 工具等）只能访问本机/执行 mount 上的文件系统。

当 agent 生成 `shell_exec: cat skills://code-review/context.md` 或
`run_script: skills://code-review/check.sh` 时，这些路径对执行侧不可见。

## 设计

### 核心：Mount URI 物化层（横切所有 tool call）

在 **`before_tool_call`** 阶段，对所有工具的所有字符串参数统一扫描 `mount_id://path` 格式，
按需物化文件到 session 本地缓存，并将参数值原地重写为本地路径。

```
Tool call 参数（任意工具）
  → 扫描所有字符串 arg，匹配 (?<id>\w+)://(?<path>[^\s"']+)
  → id 是已注册的 mount_id？
      是 → relay_service.read_text(id, path) → 写入 session cache
           重写 arg 值：skills://foo/bar.md → /tmp/agentdash-{sid}/mounts/skills/foo/bar.md
      否 → 保留原值（http://, https:// 等标准 scheme 不触碰）
```

**示例**：

| 原始 arg | 物化后 |
|---------|-------|
| `cat skills://code-review/context.md` | `cat /tmp/agentdash-abc/mounts/skills/code-review/context.md` |
| `run_script: skills://tools/check.sh` | `run_script: /tmp/agentdash-abc/mounts/skills/tools/check.sh` |
| `fetch: https://example.com/api` | ← 不处理（标准 URL scheme）|
| `path: main://src/main.rs` | ← 不处理（main 是执行 mount，直接可访问）|

### 哪些 mount 触发物化

**触发条件**：mount 没有 `Exec` 能力，或 mount 的 backend 与当前执行 mount 不同。

执行 mount（通常是 `main`）的路径不需要物化，工具已经可以直接访问。

### Session 缓存

```
/tmp/agentdash-{session_id}/mounts/{mount_id}/{relative_path}
```

- 按路径 key 做内存 + 磁盘两级 cache，同一 session 内不重复下载
- Session 结束时清理（接入 `SessionTerminal` hook）
- 单文件大小上限可配置（默认 5MB），超出时报错提示 agent 显式处理

### 复杂操作的处理边界

物化层只做**只读内容访问的透明化**。如果 agent 需要在云端脚本上做复杂操作
（执行、修改、多文件协作），**由 agent 自行显式搬运**：

```bash
# agent 自行搬运的标准流程：
read_file skills://tools/check.sh         # 读取内容
write_file main://tmp/check.sh [content]  # 写到本机
shell_exec main://. bash ./tmp/check.sh   # 在本机执行
```

这条边界保持 agent 意图的可见性，不在引擎层做透明的执行代理。

## 实施要点

- 物化逻辑放在 `agentdash-application/src/address_space/materialization.rs`（新文件）
- 接入点：`before_tool_call` 的 `ToolCallDecision::Rewrite` 路径，在 hook 评估后、实际执行前注入一个 materialize pass
- mount_id 白名单从当前 session 的 `AddressSpace` 动态构建，避免误匹配非 mount 的 `word://` 字符串
- 执行 mount 排除逻辑：检查 mount 的 `capabilities` 是否含 `Exec`，含则跳过物化

## 刻意不做

| 排除项 | 原因 |
|--------|------|
| Skill assets 预取 | 过度设计，按需物化已够 |
| 通用相对路径解析 | 需要 bash 静态分析，不可靠 |
| 写回云端 | 超出只读边界 |
| FUSE 挂载 | 运维复杂度过高 |

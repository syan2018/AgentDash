# 跨 Mount URI 统一路径解析层

> 状态：planning
> 前置依赖：`04-08-tool-execution-improvements`（before_tool_call 拦截框架）

## 问题背景

Agent 生成的工具调用参数里可能包含 `mount_id://path` 格式的 URI，
但工具实际执行时只理解本机文件系统路径。需要一个统一的解析层：

- **本地 mount**：`main://src/foo.rs` → 直接解析为真实物理路径（`/workspace/repo/src/foo.rs` 或 `C:\workspace\repo\src\foo.rs`）
- **云端 mount**：`skills://code-review/context.md` → 按需下载到 session 缓存，返回缓存路径

两种情况用同一套逻辑处理，对工具透明。

**`cp` 示例**（agent 自然写出，无需特殊处理）：
```
cp skills://code-review/check.sh main://tmp/check.sh
→ cp /tmp/agentdash-{sid}/mounts/skills/code-review/check.sh /workspace/repo/tmp/check.sh
```

## 设计

### 核心：统一 Mount URI 解析器

在 `before_tool_call` 阶段，对所有工具的所有字符串参数扫描 mount URI，统一解析为本地路径：

```rust
pub struct MountUriResolver {
    address_space: AddressSpace,
    cache: SessionMountCache,
}

impl MountUriResolver {
    /// 将 arg 里所有 mount URI 替换为本地路径
    /// 不认识的 scheme（http/https 等）原样保留
    pub async fn resolve_arg(&self, arg: &str) -> Result<String>;
}
```

**解析规则**：

```
mount_id://relative/path
  → 查找 address_space.mounts 中 id == mount_id 的 mount
  → 本地 mount（has Exec capability，backend 是当前机器）：
       root_ref + "/" + relative/path → 归一化为平台路径
  → 云端 mount（无 Exec 或 backend 不在本机）：
       relay_service.read_text(mount_id, path) → 写入 session cache
       cache_dir / mount_id / relative/path → 平台路径
  → 未知 mount_id（或标准 scheme）：原样保留
```

### Windows 兼容

URI 内部统一用 `/` 作分隔符（URL 风格），解析为本地路径时按平台转换：

```rust
fn uri_path_to_local(root_ref: &str, uri_path: &str) -> PathBuf {
    // uri_path 始终是 / 分隔
    let segments: Vec<&str> = uri_path.split('/').filter(|s| !s.is_empty()).collect();
    let mut path = PathBuf::from(root_ref);  // root_ref 已是平台路径
    for seg in segments {
        path.push(seg);  // PathBuf::push 自动处理平台分隔符
    }
    path
}
```

**Session 缓存目录（跨平台）**：

```rust
fn session_cache_dir(session_id: &str) -> PathBuf {
    // Windows: C:\Users\{user}\AppData\Local\Temp\agentdash\{session_id}
    // Unix:    /tmp/agentdash/{session_id}
    std::env::temp_dir()
        .join("agentdash")
        .join(session_id)
        .join("mounts")
}
```

**路径安全**（防 path traversal）：

```rust
// 复用现有 normalize_mount_relative_path()，已有越界检测
// URI 中的 .. 在解析前拒绝
```

### 接入点

物化 pass 在 hook 评估（`ToolCallDecision`）之后、工具实际执行之前运行，
作为 `before_tool_call` 流水线的最后一步：

```
before_tool_call
  1. HookRuntimeDelegate 评估（Allow / Deny / Rewrite）
  2. MountUriResolver pass（统一解析所有 arg 里的 mount URI）  ← 新增
  3. 执行工具
```

### 缓存管理

- 同一 session 内相同 (mount_id, path) 只下载一次（内存 HashMap 作一级 cache）
- 单文件大小上限可配置，默认 5MB，超出时不物化，返回错误提示 agent 换方式处理
- Session 结束（`SessionTerminal` hook）时清理整个 cache 目录

## 实施要点

- 新文件：`crates/agentdash-application/src/address_space/mount_uri_resolver.rs`
- mount_id 白名单从当前 session 的 `AddressSpace.mounts` 动态构建，避免误匹配普通 `word://` 字符串
- 本地 mount 判定：检查 mount capabilities 含 `Exec`，且 `backend_id` 对应当前进程所在机器
- 所有路径操作使用 `std::path::PathBuf`，禁止手动字符串拼接路径

## 刻意不做

| 排除项 | 原因 |
|--------|------|
| 相对路径（`./foo.md`）解析 | 需要 bash 静态分析，不可靠 |
| 写回云端 | 超出只读缓存边界 |
| FUSE 挂载 | 运维复杂度过高 |
| Skill assets 预取 | 按需物化已够，不需要 eager 预取 |

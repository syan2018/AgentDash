# Design — fs 工具 virtual projection 覆盖与 relay 协议补齐

> **PRD:** [./prd.md](./prd.md)

## §1 SPI grep_text 默认实现升级（R1）

### §1.1 通用 list+read+regex 算法

[mount.rs:535](../../../crates/agentdash-spi/src/platform/mount.rs#L535) 的
`grep_text` 默认实现从"forward 给 search_text"升级为：

```rust
async fn grep_text(
    &self,
    mount: &Mount,
    query: &GrepQuery,
    ctx: &MountOperationContext,
) -> Result<SearchResult, MountError> {
    // 1. list 全 mount 文件（path 字段限定起点）
    let listing = self.list(
        mount,
        &ListOptions {
            path: query.base.path.clone().unwrap_or_default(),
            pattern: None,
            recursive: true,
        },
        ctx,
    ).await?;

    // 2. 编译 regex + glob_matcher
    let mut builder = regex::RegexBuilder::new(&query.base.pattern);
    builder
        .case_insensitive(!query.base.case_sensitive)
        .multi_line(query.multiline)
        .dot_matches_new_line(query.multiline);
    let re = builder.build().map_err(|e| MountError::OperationFailed(...))?;
    let glob_matcher = ...;

    let before = query.before_lines.max(query.context_lines);
    let after = query.after_lines.max(query.context_lines);
    let max_results = query.base.max_results.unwrap_or(usize::MAX);

    // 3. 逐文件 read_text + 匹配
    let mut matches = Vec::new();
    for entry in listing.entries {
        if entry.is_dir { continue; }
        if let Some(matcher) = &glob_matcher {
            if !matcher.is_match(entry.path.as_str()) { continue; }
        }
        // 二进制跳过：attributes.content_kind == "binary"
        if entry_is_binary(&entry) { continue; }
        // 按需调 read_text；read_text 失败的条目跳过（warn 一次）
        let read = match self.read_text(mount, &entry.path, ctx).await {
            Ok(r) => r,
            Err(MountError::NotSupported(_) | MountError::NotFound(_)) => continue,
            Err(e) => return Err(e),
        };
        // 在内存跑 regex + before/after_lines 上下文
        let lines: Vec<&str> = read.content.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            if !re.is_match(line) { continue; }
            let start = idx.saturating_sub(before);
            for ctx_idx in start..idx {
                matches.push(SearchMatch { path: entry.path.clone(), line: Some((ctx_idx + 1) as u32), content: lines[ctx_idx].trim().to_string() });
            }
            matches.push(SearchMatch { path: entry.path.clone(), line: Some((idx + 1) as u32), content: line.trim().to_string() });
            let end = (idx + 1 + after).min(lines.len());
            for ctx_idx in (idx + 1)..end {
                matches.push(SearchMatch { path: entry.path.clone(), line: Some((ctx_idx + 1) as u32), content: lines[ctx_idx].trim().to_string() });
            }
            if matches.len() >= max_results {
                return Ok(SearchResult { matches, truncated: true });
            }
        }
    }
    Ok(SearchResult { matches, truncated: false })
}
```

### §1.2 性能注意

- list recursive=true 在 lifecycle journey 下可能枚举几十-几百条 virtual entry，
  每条 entry 一次 read_text RPC 调用代价不低（journey JSON 计算）。但 fs_grep
  本就是 agent 探查接口，性能瓶颈用 head_limit / glob 限制范围。
- 已有 `query.base.max_results` 短路：命中达上限即返回，避免无谓的 read。
- include_glob 在 list 后过滤 binary entries 早期跳过，进一步压缩 read 次数。

### §1.3 inline_fs 保留 override

`InlineFsMountProvider::grep_text` 现状（直接读 inline_files 表）保留，性能更优。
其它 provider 走默认。

## §2 Relay 协议字段补齐（R2）

### §2.1 ToolFileReadPayload

[tool.rs:6](../../../crates/agentdash-relay/src/protocol/tool.rs#L6) 加：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFileReadPayload {
    pub call_id: String,
    pub path: String,
    pub mount_root_ref: String,
    /// 0-based 起始行号；省略 = 从头读。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    /// 行数上限；省略 = 读到 EOF。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}
```

`#[serde(skip_serializing_if = "Option::is_none")]`：旧 backend 不会看到新字段；
新字段缺失自动 None。

### §2.2 ToolSearchPayload

```rust
pub struct ToolSearchPayload {
    pub call_id: String,
    pub mount_root_ref: String,
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default)]
    pub is_regex: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_glob: Option<String>,
    #[serde(default = "default_search_max_results")]
    pub max_results: usize,
    #[serde(default)]
    pub context_lines: usize,
    // === NEW ===
    #[serde(default = "default_case_sensitive")]
    pub case_sensitive: bool,
    #[serde(default)]
    pub multiline: bool,
    #[serde(default)]
    pub before_lines: usize,
    #[serde(default)]
    pub after_lines: usize,
}

fn default_case_sensitive() -> bool { true }
```

`#[serde(default)]` 让旧 payload 反序列化能用旧默认；新字段在序列化时永远会写出
（不带 skip_if），确保 JSON 一致。

## §3 relay_fs provider 实现（R3）

### §3.1 `read_text_range`

[crates/agentdash-api/src/mount_providers/relay_fs.rs](../../../crates/agentdash-api/src/mount_providers/relay_fs.rs)
加方法（覆盖 SPI 默认）：

```rust
async fn read_text_range(
    &self,
    mount: &Mount,
    path: &str,
    offset: usize,
    limit: Option<usize>,
    ctx: &MountOperationContext,
) -> Result<ReadResult, MountError> {
    // 构造 ToolFileReadPayload { offset: Some(offset as u64), limit: limit.map(|n| n as u64) }
    // 走 RelayMessage::RequestToolFileRead 通道
    // ResponseToolFileRead 处理同 read_text
}
```

### §3.2 `grep_text`

```rust
async fn grep_text(
    &self,
    mount: &Mount,
    query: &GrepQuery,
    ctx: &MountOperationContext,
) -> Result<SearchResult, MountError> {
    // 构造 ToolSearchPayload，把 GrepQuery 全部字段映射进去：
    //   query.base.pattern → query
    //   query.base.path → path
    //   query.base.case_sensitive → case_sensitive
    //   query.base.max_results → max_results (有默认)
    //   query.include_glob → include_glob
    //   query.context_lines → context_lines
    //   query.before_lines → before_lines
    //   query.after_lines → after_lines
    //   query.multiline → multiline
    //   is_regex 始终为 true (A7 决议)
    // ResponseToolSearch 处理同 search_text
}
```

`output_mode` 在 service 层（fs_grep tool 层）转换，protocol 不区分。

## §4 lifecycle.search_text 调整（R4）

[provider_lifecycle.rs:743](../../../crates/agentdash-application/src/vfs/provider_lifecycle.rs#L743)
现状：

```rust
async fn search_text(...) -> Result<SearchResult, MountError> {
    if path.starts_with("skills") {
        return search_projected_skill_files(...);
    }
    Ok(SearchResult { matches: vec![], truncated: false })  // ← bug
}
```

修改为：

```rust
async fn search_text(...) -> Result<SearchResult, MountError> {
    if path.starts_with("skills") {
        return search_projected_skill_files(...);
    }
    Err(MountError::NotSupported(format!(
        "lifecycle_vfs 仅在 skills 子树支持 substring search_text；\
         其他路径请用 grep_text"
    )))
}
```

效果：

- `service.search_text_extended` 在 lifecycle 非 skills 路径上会得到错误（合理 — agent
  端 fs_search 工具暂未公开接口；fs_grep 走 grep_text 路径不受影响）。
- `service.grep_text_extended` 在 lifecycle 非 inline 分支调 `provider.grep_text`，
  走 R1 升级后的 SPI 默认实现，自动 list+read+regex 覆盖 virtual projection。

## §5 测试矩阵

### §5.1 集成：journey state grep + range read（R5）

落 [provider_lifecycle.rs](../../../crates/agentdash-application/src/vfs/provider_lifecycle.rs)
的测试模块（如果没有则新建）。Mock 一个最简 LifecycleRunRepository + Journey trait
返回固定的 step state + 一个 5KB tool-call stdout。

测试：

- T1 `read_text_range` 在 lifecycle virtual `tool-calls/{id}/stdout`：
  offset=100, limit=50 ⇒ 返回 100-149 行。
- T2 `grep_text` 在 lifecycle virtual `tool-calls/{id}` 路径：
  pattern="error.*" ⇒ 找到对应行 + 行号正确。

### §5.2 canvas / skill_asset grep_text（R6）

各落一项 inline test：

- canvas grep_text 用 regex 正确匹配 + include_glob 过滤。
- skill_asset grep_text 用 context_lines 拿到上下文。

### §5.3 relay 协议序列化（R6）

落 [crates/agentdash-relay/src/protocol/tool.rs](../../../crates/agentdash-relay/src/protocol/tool.rs)
两项 serde 测试：

- ToolFileReadPayload 含 offset/limit 序列化 + 反序列化 round-trip。
- ToolSearchPayload 含全部新字段 round-trip + 旧 JSON（缺新字段）反序列化默认值
  正确。

## §6 决策矩阵

| ID | 决策 | 状态 |
|----|------|------|
| D1 | SPI grep_text 默认实现 = 通用 list+read+regex | accept |
| D2 | inline_fs 保留 grep_text override（性能） | accept |
| D3 | lifecycle.search_text 非 skills 路径返回 NotSupported（不做空集合） | accept |
| D4 | ToolFileReadPayload offset/limit 用 Option<u64>（兼容旧远端） | accept |
| D5 | ToolSearchPayload 新字段全 `#[serde(default)]` | accept |
| D6 | relay_fs provider 实现 read_text_range / grep_text 双方法 | accept |
| D7 | grep_text 默认实现的二进制跳过：attributes.content_kind == "binary" | accept |
| D8 | grep_text 默认实现 read_text 失败（NotFound/NotSupported）的条目跳过，不中断 | accept（容错） |
| D9 | 远端 backend 实现新字段不在本任务范围 | accept |

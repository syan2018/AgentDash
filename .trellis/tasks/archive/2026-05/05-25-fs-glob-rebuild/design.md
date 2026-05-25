# Design — fs_glob 重做

> **Parent:** [05-25-fs-tools-optimization-review](../05-25-fs-tools-optimization-review/design.md)
> **PRD:** [./prd.md](./prd.md)（含 CC 参考路径）

## §1 范围

把 [fs_glob](../../../crates/agentdash-application/src/vfs/tools/fs/glob.rs)
schema + 行为升级到 CC GlobTool 对齐：

- Schema：去 `recursive`（递归通过 `**` 表达），pattern 必填且始终 glob，
  去 substring fallback；新增 `max_results`（默认 100）。
- mtime desc 排序（用 RuntimeFileEntry.modified_at；缺失 fallback 0 + path 二级排序）。
- 输出去 `[dir]/[file]` 前缀，目录用 trailing slash 表达（CC 风格）。
- 默认上限 100；超出 ⇒ truncate + 提示。

**不在范围：** SPI 改动；ripgrep 后端；mount 边界跨 mount 的全局 glob。

## §2 Schema

```rust
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FsGlobParams {
    /// Glob pattern. Required. Always treated as glob (no substring fallback).
    /// 用 `*` 表示当前目录所有；`**/foo` 表示递归匹配。
    pub pattern: String,
    /// mount-rooted path 起点。省略 = mount 根。
    pub path: Option<String>,
    /// 命中条目上限。默认 100；`0` = 无限。
    pub max_results: Option<usize>,
}
```

`deny_unknown_fields` 让旧 `recursive` 字段即时报错（A8 决议）；
旧调用 pattern 缺失 ⇒ serde 报错。

## §3 实施流程

```
fs_glob 入参
  ├─ deny_unknown_fields 校验
  ├─ resolve_uri_path
  ├─ service.list(target, ListOptions { path, pattern: Some(pattern), recursive: detect_recursion(pattern) })
  │     注：现有 list 接口接受 pattern + recursive；保留兼容，
  │     pattern 是否含 `**` 决定是否递归。
  ├─ filter is_vcs_path（与 fs-grep 共享，硬编码 6 目录）
  ├─ sort_by_key(reverse(modified_at).unwrap_or(0)) + path 二级排序
  ├─ take(max_results.unwrap_or(100)) — 0 = 无限
  ├─ format: 文件 → `path`，目录 → `path/`
  └─ truncated 后缀如有
```

### §3.1 pattern 是否触发递归

CC GlobTool 的语义：递归用 `**` 显式表达。
我们的 `service.list` 接口现状有 `recursive: bool` 参数。

**决策：** 在 fs_glob tool 层，根据 pattern 是否含 `**` 推断 recursive：

```rust
let recursive = pattern.contains("**");
```

这样调用方不需要管 recursive 字段，pattern `*.rs` 仅当前目录，`**/*.rs` 全 mount。

### §3.2 mtime desc 排序

```rust
entries.sort_by(|a, b| {
    let a_mtime = a.modified_at.unwrap_or(0);
    let b_mtime = b.modified_at.unwrap_or(0);
    b_mtime.cmp(&a_mtime).then_with(|| a.path.cmp(&b.path))
});
```

primary key = -modified_at（desc）；secondary = path（asc 字典序，确保
确定性）。modified_at 缺失填 0（沉底）。

### §3.3 输出格式

```rust
entries
    .iter()
    .map(|e| {
        let path = e.path.replace('\\', "/");
        if e.is_dir {
            format!("{path}/")
        } else {
            path
        }
    })
    .collect::<Vec<_>>()
    .join("\n")
```

去除 `[dir]` / `[file]` 前缀，目录尾部加 `/`。

### §3.4 truncated 提示

`max_results.unwrap_or(100)` —— `0 = 无限`：

```rust
let cap = match params.max_results {
    Some(0) => usize::MAX,
    Some(n) => n,
    None => 100,
};
let truncated = entries.len() > cap;
let entries: Vec<_> = entries.into_iter().take(cap).collect();
```

truncated ⇒ 末尾追加：
`(N more entries; refine pattern or raise max_results)`。

## §4 prompt 措辞

参考 [GlobTool/prompt.ts](../../../references/claude-code/src/tools/GlobTool/prompt.ts)：

```
Fast file pattern matching using glob patterns.

Usage:
- The pattern parameter is required and always interpreted as a glob.
- Use `*` for current directory; `**/foo` for recursive match.
- Use `path` to scope the search to a sub-directory.
- Returns paths sorted by modification time (newest first), then alphabetically.
- Directories are shown with a trailing slash (`src/utils/`).
- Default limit: 100 entries; pass max_results: 0 for unlimited.
- VCS directories (.git, .svn, ...) are excluded automatically.
```

## §5 测试矩阵

- T1 schema：旧 `recursive` ⇒ InvalidArguments；缺失 `pattern` ⇒ InvalidArguments。
- T2 pattern 始终 glob：`pattern: "foo"` 仅匹配文件名 `foo`，不匹配 `foobar`。
- T3 递归 inferred：`pattern: "**/*.rs"` 全 mount 匹配；`pattern: "*.rs"` 仅根目录。
- T4 mtime desc：构造 a.rs (mtime=1)、b.rs (mtime=2)、c.rs (mtime=3) ⇒ 顺序 c,b,a。
- T5 mtime 缺失：modified_at = None 的两条按 path asc 兜底。
- T6 默认上限：构造 200 个文件 ⇒ 100 + truncated 后缀。
- T7 max_results = 0：构造 200 个文件 ⇒ 全部返回，无 truncated。
- T8 输出格式：目录 `src/utils/`，文件无后缀；无 `[dir]/[file]` 前缀。
- T9 VCS 排除：含 `.git/HEAD` 不出现。

## §6 决策矩阵

| ID | 决策 | 状态 |
|----|------|------|
| D1 | recursive 字段去除，递归通过 `**` 表达 | accept（CC 对齐 + A8） |
| D2 | pattern 必填且始终 glob，无 substring fallback | accept |
| D3 | 默认 max_results = 100；`0` = 无限 | accept |
| D4 | mtime desc 排序，缺失沉底 + path 二级排序 | accept |
| D5 | 输出去前缀 + 目录 trailing slash | accept |
| D6 | VCS 黑名单复用 fs_grep 的 is_vcs_path（已在 service 层）；fs_glob 在 tool 层用 same helper | accept |
| D7 | recursive 自动从 pattern 推断（含 `**`） | accept |
| D8 | service.list 接口不动（仍带 recursive 参数）；tool 层做转换 | accept |

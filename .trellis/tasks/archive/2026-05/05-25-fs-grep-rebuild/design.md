# Design — fs_grep 重做

> **Parent:** [05-25-fs-tools-optimization-review](../05-25-fs-tools-optimization-review/design.md)
> **PRD:** [./prd.md](./prd.md)（含 CC 参考路径）

## §1 范围

把 [fs_grep](../../../crates/agentdash-application/src/vfs/tools/fs/grep.rs)
schema + 行为升级到 CC GrepTool 对齐：

- Schema 改名（query/regex/include/max_results/context_lines → pattern/glob/
  type/output_mode/before_context/after_context/context/case_insensitive/
  line_numbers/multiline/head_limit/offset）。
- pattern 始终视为正则（A7 决议），无 `regex` 字段。
- output_mode 三档：Content / FilesWithMatches（**新默认**） / Count，
  在 tool 层处理（service 仍返回完整命中）。
- VCS 黑名单：6 个目录硬编码（A3 决议）。
- 长行裁剪：MAX_LINE_LEN = 500，超长 → trim + `...(truncated)`。
- type 快捷键：10 种语言映射。
- head_limit + offset 分页（CC GrepTool 也是这个设计）。

**不在范围：** SPI 改动；ripgrep 后端；GrepQuery trait split（→ FU#1）。

## §2 Schema（breaking）

```rust
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FsGrepParams {
    /// 正则表达式 pattern。始终按 regex 解释（与 CC 对齐；无 fixed-string 模式）。
    pub pattern: String,
    /// mount://path 起点；省略 = 全 mount。
    pub path: Option<String>,
    /// 文件 glob（如 `*.rs`、`src/**/*.ts`）。与 type 叠加 = 并集。
    pub glob: Option<String>,
    /// 语言快捷键（rust/js/ts/py/go/java/c/cpp/cs/rb）。
    #[serde(rename = "type")]
    pub type_: Option<String>,
    /// `Content` / `FilesWithMatches`（默认） / `Count`。
    pub output_mode: Option<OutputMode>,
    /// `-i` 等价。默认 false。
    #[serde(default)]
    pub case_insensitive: bool,
    /// `-n` 等价。默认 true。
    #[serde(default = "default_true")]
    pub line_numbers: bool,
    /// `-U` multiline。默认 false。
    #[serde(default)]
    pub multiline: bool,
    /// `-B` 等价。
    pub before_context: Option<usize>,
    /// `-A` 等价。
    pub after_context: Option<usize>,
    /// `-C` 等价；与 `before_context/after_context` 同时设置时取 max。
    pub context: Option<usize>,
    /// 命中行（或文件）上限。默认 250；`0` = 无限。
    pub head_limit: Option<usize>,
    /// 与 head_limit 配合分页。默认 0。
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    Content,
    #[default]
    FilesWithMatches,
    Count,
}
```

`deny_unknown_fields` 让旧字段（`query/regex/include/max_results/context_lines`）
即时报错。

## §3 type 快捷键

```rust
const LANG_EXTENSIONS: &[(&str, &[&str])] = &[
    ("js",   &["js","jsx","mjs","cjs"]),
    ("ts",   &["ts","tsx","mts","cts"]),
    ("py",   &["py","pyi"]),
    ("rust", &["rs"]),
    ("go",   &["go"]),
    ("java", &["java"]),
    ("c",    &["c","h"]),
    ("cpp",  &["cc","cpp","cxx","hpp","hxx"]),
    ("cs",   &["cs"]),
    ("rb",   &["rb"]),
];
```

`type: "rust"` ⇒ 翻译为 glob `**/*.rs`。type + glob 同时给 ⇒ 取**并集**：
combined glob = `{user_glob,**/*.{ext1,ext2,...}}`（用 globset alternative）。

## §4 VCS 黑名单（A3 决议 = 硬编码）

```rust
const VCS_EXCLUDE_DIRS: &[&str] = &[".git", ".svn", ".hg", ".bzr", ".jj", ".sl"];
```

注入到 `TextSearchParams` 的逻辑：在 service 层（search_text_extended /
search_inline）的 path 过滤里加一道 segment 检查 — 如果 path 中任一 segment
等于黑名单成员，跳过该文件。

**位置：** `relay_service::search_inline` + `RelayVfsService` 把 VCS exclude
注入 `SearchQuery.include_glob`（用 negative glob `!**/.git/**` 等）。

**问题：** globset 的 negative pattern 语法是 `Glob::new("!...")` 还是
GlobSet 的 `add_negative`？看 globset doc：标准做法是建 `GlobSet` 时
`new_with_negation` — 但 SPI 现状用单个 `Glob`。

**简化决策：** VCS 排除在 service 层硬过滤（segment 包含黑名单 ⇒ skip），
不依赖 glob 系统。这样 4 个 provider 不需要逐个适配。

## §5 长行裁剪

```rust
const MAX_LINE_LEN: usize = 500;
const TRUNCATE_SUFFIX: &str = "...(truncated)";

fn trim_line(line: &str) -> String {
    if line.chars().count() <= MAX_LINE_LEN {
        line.to_string()
    } else {
        let head: String = line.chars().take(MAX_LINE_LEN).collect();
        format!("{}{}", head, TRUNCATE_SUFFIX)
    }
}
```

实施位置：search_inline 在格式化 hit 时调；其他 provider 路径在
`relay_service::search_text_extended` 收到 SearchResult 后统一过滤。

**注意：** 用 char 计数避免 UTF-8 切割错误。

## §6 output_mode 三档（tool 层方案 A）

service 仍按 Content 返回完整命中行。tool 层在序列化前转换：

```rust
match params.output_mode.unwrap_or_default() {
    OutputMode::Content => format_content(hits, line_numbers),
    OutputMode::FilesWithMatches => {
        let unique: BTreeSet<_> = hits.iter().filter_map(parse_path).collect();
        unique.into_iter().collect::<Vec<_>>().join("\n")
    }
    OutputMode::Count => {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for hit in hits {
            if let Some(p) = parse_path(&hit) {
                *counts.entry(p).or_insert(0) += 1;
            }
        }
        counts.iter().map(|(p, c)| format!("{p}:{c}")).collect::<Vec<_>>().join("\n")
    }
}
```

`parse_path(&hit)` 解析 `path:line:content` 形式拿到 path。

## §7 head_limit + offset 分页

- 默认 `head_limit = 250`，`0 = 无限`。
- `offset = 0` 时直接对 hits 切片：`hits.into_iter().skip(offset).take(head_limit)`。
- service 层的 `max_results` 不能简单 = head_limit + offset：因为 service
  按 max_results 截断后，offset 的精确度依赖完整命中集。
  - **决策：** service 层传 `max_results = head_limit + offset`（含 buffer），
    tool 层再 skip + take。这意味着 tool 层依赖 service 返回相对充足的命中。
  - `head_limit = 0`（无限）⇒ service 层传一个大数（如 50000）。

## §8 流程

```
fs_grep 入参
  ├─ deny_unknown_fields 校验
  ├─ resolve_uri_path
  ├─ build_combined_glob(glob, type) → effective_glob
  ├─ build_search_params {
  │     pattern,
  │     case_sensitive: !case_insensitive,
  │     before_lines: max(before_context, context),
  │     after_lines: max(after_context, context),
  │     multiline,
  │     include_glob: effective_glob,
  │     max_results: head_limit.unwrap_or(250) + offset,
  │     output_mode: Content (always; tool layer handles real mode),
  │  }
  ├─ service.search_text_extended(...)
  ├─ filter VCS_EXCLUDE_DIRS（path segment 包含黑名单 ⇒ skip）
  ├─ filter MAX_LINE_LEN（超长 line 裁剪 + 后缀）
  ├─ skip(offset).take(head_limit)
  ├─ format by output_mode
  └─ output + "(truncated)" 后缀如有
```

## §9 测试矩阵

- T1 schema：旧 `query` 字段 ⇒ InvalidArguments。
- T2 pattern 始终正则：`pattern: "func.*foo"` 匹配 `funcXfoo`；
  `pattern: "function foo"` 匹配字面值（无元字符）。
- T3 output_mode = Content：返回 `path:line:content`。
- T4 output_mode = FilesWithMatches（默认）：去重 path 列表。
- T5 output_mode = Count：每文件计数。
- T6 case_insensitive：`pattern: "FOO"` + `case_insensitive: true` 匹配 `foo`。
- T7 multiline：`(?s)`-style；测试用 `pattern: "alpha.beta"` 跨行匹配。
- T8 before_context / after_context：构造 5 行文件，命中第 3 行 + before=1 +
  after=1 ⇒ 输出含第 2/3/4 行。
- T9 type 快捷键：`type: "rust"` 仅命中 .rs 文件。
- T10 type + glob 并集：`type: "rust", glob: "*.toml"` 命中 .rs 和 .toml。
- T11 head_limit + offset 分页：head_limit=2, offset=2 ⇒ 跳过前 2 个。
- T12 VCS 排除：含 `.git/HEAD` 和 `src/main.rs` 的 mount，匹配后无 .git 路径。
- T13 长行裁剪：含 5KB 单行，命中 line 长度 ≤ 500 + truncated 后缀。

测试用 inline provider mock，因为它已支持 regex（vfs-search-spi-fix 中
inline 实际还是 substring，需要在 fs-grep-rebuild 中升级到 regex）。

## §10 inline provider 升级

[provider_inline.rs:141-187](../../../crates/agentdash-application/src/vfs/provider_inline.rs#L141-L187)
当前是 substring。本任务把它升级为：

```rust
let re = if query.is_regex {
    let mut builder = regex::RegexBuilder::new(&query.pattern);
    builder
        .case_insensitive(!query.case_sensitive)
        .multi_line(query.multiline)
        .dot_matches_new_line(query.multiline);
    Some(builder.build().map_err(|e| MountError::OperationFailed(e.to_string()))?)
} else {
    None
};
// match re.is_match(line) or substring
```

include_glob 用 globset 过滤；before/after_lines 在命中后用相邻行追加。

VCS 排除在 service 层（不在 inline 内），inline 不需要管。

## §11 决策矩阵

| ID | 决策 | 状态 |
|----|------|------|
| D1 | output_mode 默认 = FilesWithMatches | accept（CC 对齐 + token 节约） |
| D2 | A3 VCS 黑名单 = 硬编码 6 个目录（不可配置） | accept |
| D3 | 长行裁剪在 service 层而非 provider 层 | accept（统一逻辑，4 provider 不重复） |
| D4 | head_limit + offset = service 层传 max_results = head_limit + offset | accept |
| D5 | type + glob 并集，glob 优先级未定 ⇒ 任一匹配即纳入 | accept |
| D6 | inline provider 升级为 regex（在 vfs-search-spi-fix 后做） | accept |
| D7 | output_mode 转换在 tool 层（Method A） | accept（PRD 决议） |
| D8 | line_numbers = false 时仅省略输出，不影响 service 调用 | accept |
| D9 | head_limit = 0 ⇒ service 层传 50000 | accept（合理上限，不会 OOM） |

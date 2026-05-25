# Design — fs_read / fs_grep / fs_glob 工具优化评估

> 本文是 [prd.md](./prd.md) 的核实产物。每条候选项三段式：**成立性 / 收益 / 成本**，
> 末尾给出决策矩阵 + 建议拆分的 child task。

## 0. 评估基线 — 与 Claude Code 对齐（项目级原则）

> 本任务的所有评估**默认以 Claude Code 同名工具为基线**：现状偏离 CC 的部分
> 必须在 design.md 中列入"对齐 diff"，不在故意偏离白名单内的偏离一律视为
> 待修复 debt。这条原则记在 [feedback_fs_tools_align_with_claude_code](../../../../../.claude/projects/d--ABCTools-Dev-AgentDashboard/memory/feedback_fs_tools_align_with_claude_code.md)。

### 对齐 diff 表（参数 / 输出 / 语义）

#### fs_read vs FileReadTool

| 维度 | 当前 | CC | 决策 |
|------|------|-----|------|
| 路径参数 | `path` | `file_path` | **保留 `path`**（mount://path 协议项目独有，不可对齐） |
| 起始行 | `start_line: usize` (1-based) | `offset: usize` (1-based) | **改名为 `offset`** |
| 结束行 | `end_line: usize` (inclusive) | `limit: usize` (count) | **改语义为 `limit`**（offset+limit 是分页友好语义，offset+end_line 容易 off-by-one） |
| 大文件上限 | 无 | 有（25K tokens / `MaxFileReadTokenExceededError`） | **加上限**（P0#1 主条目） |
| 按 range 真读 | 无（全文加载后 slice） | 有（readFileInRange） | **真按 range 读**（升级 P0#1 range 分支） |
| Dedup（同 path/range/未变） | 无 | 有（readFileState） | **加 dedup**（P0#2） |
| ENOENT 友好提示 | 无（透传 NotFound） | 有（findSimilarFile + suggestPathUnderCwd） | **加友好提示**（升级 P2#8） |
| 输出行号格式 | `{:>4} \| {content}` | `addLineNumbers`（CC 内部） | 保留现状（CC 内部细节，可后续微调） |
| 二进制图片返回 | ContentPart::Image + 文本元数据 | image base64 block | 已对齐 |
| Notebook / PDF | 无 | 有 | **明确不对齐**（mount 抽象层缺 PDF 支持是后续课题，本任务不做） |

#### fs_grep vs GrepTool

| 维度 | 当前 | CC | 决策 |
|------|------|-----|------|
| 查询参数 | `query: String` | `pattern: String` | **改名为 `pattern`** |
| 正则开关 | `regex: bool`（默认 false） | 无（pattern 始终是正则） | **去除 `regex` 字段**，pattern 一律视为正则；literal 用法由 LLM 自己 escape |
| 包含过滤 | `include: Option<String>` | `glob: Option<String>` | **改名为 `glob`** |
| 文件类型 | 无 | `type: js\|py\|rust\|...` | **加 `type` 字段**（升级 P2#10，最小集 5–10 种） |
| 上限参数 | `max_results: Option<usize>` | `head_limit: Option<usize>`（默认 250、`0` = 无限） | **改名 + 改默认值**为 `head_limit` |
| 偏移参数 | 无 | `offset: number` | **加 `offset`**（与 head_limit 配合分页） |
| Context | `context_lines: usize`（对称） | `-A`/`-B`/`-C`/`context` 四字段 | **加全部四个字段**（升级 P1#6） |
| Case insensitive | 无 | `-i: bool` | **加 `-i`**（P1#6） |
| Multiline | 无 | `multiline: bool` | **加 `multiline`**（P1#6） |
| 行号显示 | 始终显示 | `-n: bool`（默认 true） | **加 `-n`** |
| Output mode | 始终 content | `output_mode: content\|files_with_matches\|count`（默认 files_with_matches） | **加 `output_mode`**（P0#3） |
| VCS 自动排除 | 无 | 6 个 VCS 目录硬编码 `!**/.git` 等 | **加 VCS 黑名单**（P1#5） |
| 长行裁剪 | 无 | `--max-columns 500` | **加长行裁剪**（P1#4） |
| 命中行格式 | `path:line: content` | `path:line:content`（rg 风格无空格） | 微调统一 |

#### fs_glob vs GlobTool

| 维度 | 当前 | CC | 决策 |
|------|------|-----|------|
| pattern 参数 | `pattern: Option<String>` | `pattern: String`（必填） | **改为必填**（取消"无 pattern 列出目录"语义；列目录用 `**` 表达） |
| 路径参数 | `path: Option<String>` | `path: Option<String>` | 已对齐 |
| 递归字段 | `recursive: Option<bool>` | 无（用 `**` 表达） | **去除 `recursive`**（语义改由 pattern 控制） |
| pattern 退化为 substring | 是（无 glob 字符时） | 否 | **去除 substring 退化**，统一 glob 语义 |
| max_results 上限 | 无 | 100（hardcoded `globLimits`） | **加默认 100**（P1#7） |
| Truncated 标志 | 无 | `truncated: bool` 输出 | **加 truncated 输出**（P1#7） |
| 排序 | 无（provider 顺序） | mtime desc，缺失 fallback path | **加 mtime 排序**（P1#7） |
| 输出格式 | `[dir] path` / `[file] path` | 路径列表（目录用 trailing slash） | **改为 trailing slash**（P2#9 重新分类为 accept），节省每行 6 字节 + 与 CC 输出对齐 |

### 命名分层：tool name 与 SPI 名分开管理

**双层分离**（用户对齐的项目原则）：

| 层 | 命名风格 | 例子 | 设计目标 |
|----|---------|------|---------|
| Tool name（LLM-facing） | unix 工业习语 | `fs_grep` / `fs_glob` / `fs_read` | 识别度高，CC 同名工具对齐，LLM 训练数据覆盖好 |
| Service / SPI（工程 facing） | 语义本质 | `search_text` / `list` / `read_text`；`SearchQuery` / `ListOptions` / `ReadResult` | 通用、可扩展（未来加 vector / semantic search 复用 SearchQuery） |

**对照 CC**：CC 内部函数也用 `ripGrep()` / `glob()` / `readFileInRange()`——
具体实现层不对应 tool name，与我们一致。

**承认的 SPI 设计泄漏 + follow-up 处理：**

`SearchQuery` 计划加 `is_regex` / `include_glob` / `context_lines` /
`multiline` / `output_mode`——这些**事实上是 grep 特化**，让通用接口承担
了具体工具需求。

**用户决议（brainstorm 阶段）：方向上拆 `GrepQuery extends SearchQuery`，
但本任务的 `vfs-search-spi-fix` 不做拆分，作为 follow-up 单独开 task。**

理由：

- 拆 trait 让 SPI 改动倍增（4 provider × 2 套接口），与"先把现状对齐 CC"
  优先级冲突。
- 当前任务已是大规模重构（3 个 rebuild），再加 trait split 风险叠加。

**承诺的 follow-up（决策矩阵已列）：** `vfs-grep-query-split`，在
`vfs-search-spi-fix` + 3 rebuild 都 merge 后开。目标形态：

```rust
pub struct SearchQuery {
    pub pattern: String,
    pub path: Option<String>,
    pub max_results: Option<usize>,
    pub case_sensitive: bool,
}

pub struct GrepQuery {
    pub base: SearchQuery,
    pub is_regex: bool,
    pub include_glob: Option<String>,
    pub context_lines: usize,
    pub before_lines: usize,
    pub after_lines: usize,
    pub multiline: bool,
    pub output_mode: OutputMode,
}

trait MountProvider {
    async fn search_text(...) -> SearchResult;  // 通用搜索
    async fn grep_text(&self, mount: &Mount, query: &GrepQuery, ctx: &Ctx)
        -> Result<GrepResult, MountError> {
        // default: 转发到 search_text，丢弃 grep-specific 字段（带 warning）
    }
}
```

### 故意偏离 CC 的白名单（项目独有需求）

- **mount_id:// 协议**：CC 是单根目录 cwd 模型，我们是多 provider 多 mount。
  `path` 参数保留 `mount://path` 形式不对齐 CC 的 `file_path`。
- **fs_read 对二进制图片返回 `ContentPart::Image`**：CC 也用 image base64 block，
  但我们的 mount 抽象多了 `mime_type` 显式传递。已基本对齐，差异在元数据呈现。
- **不引入 PDF / notebook 解析**：与 mount 抽象层不对齐，引入成本 >> 收益，
  本任务范围之外。

---

## 0bis. 核实阶段的关键发现

核实阶段读了 [relay_service.rs](../../../crates/agentdash-application/src/vfs/relay_service.rs)、4 个
provider 与 [agentdash-spi/src/platform/mount.rs](../../../crates/agentdash-spi/src/platform/mount.rs)
后，识别出 PRD 没列入但**会显著影响可行性**的事实：

### F1. `SearchQuery` 字段不全 → `TextSearchParams` 三个字段被静默丢弃

`crates/agentdash-spi/src/platform/mount.rs:196-201` 中：

```rust
pub struct SearchQuery {
    pub pattern: String,
    pub path: Option<String>,
    pub case_sensitive: bool,
    pub max_results: Option<usize>,
}
```

而 `relay_service::search_text_extended` 的非 inline 分支
（`relay_service.rs:724-757`）只把 `pattern`/`path`/`max_results` 透传，
**完全丢弃 `is_regex` / `include_glob` / `context_lines`**。意思是：

- `fs_grep` 在 canvas / lifecycle / skill_asset / relay_fs mount 上传 `regex=true` **无效**。
- `include` glob 在所有非 inline mount 上**无效**。
- `context_lines` 在所有非 inline mount 上**无效**。

inline 路径（`search_inline`）也忽略 `include_glob`（`relay_service.rs:810-848`
没引用 `params.include_glob`），但实现了 regex 与 context_lines。

**这是先于 P0/P1 的 bug 类问题**，是 P1 #5（VCS 黑名单）和 P1 #6（case-insensitive
等开关）能否落地的前置依赖。

### F2. `ReadResult` 没有 mtime/version_token

`mount.rs:45-49`：

```rust
pub struct ReadResult {
    pub path: String,
    pub content: String,
    pub attributes: Option<Map<String, Value>>,
}
```

`RuntimeFileEntry` 有 `modified_at: Option<i64>`，但 `ReadResult` 没有。
**dedup 缓存的 invalidation key 没有现成数据来源**——每次 dedup 命中前必须
额外调一次 `provider.stat()`，对 canvas/inline/skill_asset 这些 in-DB
provider 而言 `modified_at` 经常是 `None`。

### F3. 非 inline 分支的 `truncated` 永远 false

`relay_service.rs:757` 在 provider 路径直接返回 `(hits, false)`，
不管 provider 实际是否截断。`SearchResult` 也没有 `truncated` 字段。

### F4. provider.read_text 是"整文件"语义

4 个 provider 的 `read_text(mount, path, ctx)` 都返回完整 content，没有按
range/byte/line 读取的接口。`fs_read` 工具层做行 slice 之前必然全量加载。

### F5. agent 进程内**没有** readFileState 等价物

grep 跑出的 file_state 命中（`crates/agentdash-application/...`）都是 backend
profile/repo 状态，不是 Claude Code 那种 per-tool dedup 状态。**dedup 缓存
要从 0 实现**。

### F6. `is_dir` 的标记现在已经在 `fs_glob` 输出中（PRD #9 的前提）

`glob.rs:106` 输出 `[dir] / [file]` 前缀。Claude Code 不带前缀但可以从
trailing slash 推断目录。我们这边的前缀是显式信息，不是噪音——P2 #9 应直接
**reject**。

---

## 1. 候选项核实（P0）

### P0 #1 — fs_read 全文加载

**成立性：✅ 完全成立。** [read.rs:108-119](../../../crates/agentdash-application/src/vfs/tools/fs/read.rs#L108-L119)
`result.content.lines().enumerate().filter(...)`，任何 `start_line/end_line`
组合都先加载全文。叠加 F4：底层 provider 也是整文件语义。

**收益：高 / 中。**

- 真实 mount 情况：canvas/inline/skill_asset 单文件普遍 < 1MB（DB 存储），
  收益小；lifecycle/relay_fs 在真实工程目录下可能命中很大文件
  （日志、生成代码、bundle），收益高。
- 真正的 token 浪费场景：LLM 误读大文件（lockfile、dist），如果没有上限
  会一次性把整个上下文窗口塞满。
- **如果只做"加上限拒绝"而不做"按 range 读"，仍能拦掉 99% 的 token 灾难**，
  因为 LLM 看到拒绝信息会自己分段读。

**成本：**

- **仅加上限**（推荐）：低 — 只改 `read.rs`，不改 SPI / provider。约 30-50 LOC。
- **按 range 读**：中-高 — 要在 SPI `MountProvider` 加 `read_text_range`
  默认实现 + 4 个 provider 各自重写优化（lifecycle/relay_fs 走真实文件 IO，
  canvas/inline 在内存里 slice）。约 200-400 LOC + SPI 兼容性影响。

**决策：accept（上限 + 真按 range 读，一并做）。**

> **Updated（用户反馈）：** 之前妥协为"仅加上限"是错的。理由：
> 1. range 读本就是 fs_read 与 fs_grep 配合使用的标准用法（grep 拿到 line
>    后用 offset/limit 精读上下文），CC 这条链路是明确支持的。
> 2. 与 CC 对齐基线（见 §0）要求 `offset/limit` 真按 range 读，不允许
>    "全文加载后 slice"的草率实现。
> 3. canvas/inline 收益看似为 0，但接入统一 SPI 后行为可预测——避免"看起来
>    支持 range 实际上没省内存"的隐性陷阱。

**落到 child task：** `fs-read-rebuild`（与 #2 + P2#8 合并）。

---

### P0 #2 — fs_read 缺 dedup

**成立性：✅ 完全成立。** F5 已确认 agent 进程内无现成 readFileState。

**收益：高。** Claude Code BQ 数据 ~18% Read 命中，量级值得做。

**成本：中-高。**

关键卡点是 F2：dedup 的 invalidation key 用什么？

| 方案 | 实现 | 适用 provider | 假阳性风险 |
|------|------|---------------|------------|
| A. mtime（来自 stat） | 命中前 stat 一次拿 modified_at | lifecycle / relay_fs / skill_asset | canvas/inline 上 modified_at 常为 None → 退化为永不命中 |
| B. content hash | 命中前读全文 hash | 所有 | 退化为"读两遍"，比不 dedup 更慢 |
| C. ReadResult 加 `version_token: Option<String>` | provider 自己产生（fs 用 mtime+size、in-DB 用 revision/版本号） | 所有 | 需要 SPI 扩展；provider 各自实现 |
| D. 组合：版本号优先，mtime 次之，否则禁用 dedup | C 的渐进版 | 所有 | 兜底为不 dedup，零假阳性 |

**推荐 D**。SPI 扩展为：

```rust
pub struct ReadResult {
    pub path: String,
    pub content: String,
    pub attributes: Option<Map<String, Value>>,
    pub version_token: Option<String>,  // NEW — opaque to caller
}
```

provider 实现：
- `provider_lifecycle` / 真实 fs：`format!("{mtime}:{size}")`
- `provider_canvas`：用 canvas 的 `version_id` 字段
- `provider_inline`：用 inline_files 表的 revision
- `provider_skill_asset`：用 skill 的 updated_at

工具层：缓存 key = `(mount_id, path, range)`，value = `(version_token, response_summary)`。
命中且 version_token 一致 → 返回短桩 `"file unchanged since previous read"`。

**LOC 预估：** SPI 扩展 + 4 provider 各填一行 ≈ 30 LOC；FsReadTool dedup 逻辑 ≈ 80 LOC。

**决策：accept（方案 D）。** 需要拆出 SPI 扩展作为前置步骤。

**落到 child task：** `fs-read-limits-and-dedup`（与 #1 合并）。前置：SPI 扩展
作为本任务的第一步（包含在同一个 child task 内）。

---

### P0 #3 — fs_grep 缺 `output_mode`

**成立性：✅ 完全成立。**

**收益：高。** "符号在哪用过"占探针 grep 的大头，文件名列表足够。

**成本：依方案而定。**

#### 方案对比表

| 维度 | A. Tool 层去重 | B. Service 层原生支持 |
|------|---------------|-----------------------|
| 实现位置 | `fs_grep` `execute()` 在结果上 dedup | `search_text_extended` 接受 `output_mode`，提早 break 内层循环 |
| SPI 改动 | 无 | `SearchQuery` 加 `output_mode` 字段 + 每 provider search_text 处理 |
| Provider 改动 | 无 | 4 个 provider 都要更新（虽然只有 inline 真正受益于早停） |
| 早停增益 | 无（仍扫完所有命中行） | 有（命中即跳到下一文件） |
| 实施 LOC | ~20 LOC | ~120 LOC + 4 provider 改动 + 测试 |
| 风险 | 0 | 中（SPI 兼容、provider 各自语义） |
| 推荐 | ✅（先做） | defer |

**理由：** 实际 grep 调用里命中行通常 < 1000 行，扫完成本可忽略；token 节省
完全发生在序列化阶段。Tool 层去重已经能拿到 95% 的收益，且改动局限在一个文件。
Service 层方案 defer 到有 profiler 数据证实早停收益值得后再做。

**决策：accept（方案 A）。**

**落到 child task：** `fs-grep-output-modes`。

---

## 2. 候选项核实（P1）

### P0/P1 之前的前置 bug：F1

> **必须做。** 是 P1 #5/#6 的强依赖。

把 `SearchQuery` 扩展到与 `TextSearchParams` 对齐：

```rust
pub struct SearchQuery {
    pub pattern: String,
    pub path: Option<String>,
    pub case_sensitive: bool,
    pub max_results: Option<usize>,
    pub is_regex: bool,            // NEW
    pub include_glob: Option<String>,  // NEW
    pub context_lines: usize,       // NEW
}
```

并修复 `relay_service::search_text_extended` 的 provider 分支：把这 3 个
字段透传给 provider。**3 个 provider 现在没用这些字段** → 默认行为不变；
4 个 provider 各自更新 search_text 实现以使用这些字段（≈ 30 LOC × 4）。

`SearchResult` 加 `truncated: bool` 解决 F3。

**落到 child task：** `vfs-search-contract-fix`（前置任务，所有 P1 依赖它）。

---

### P1 #4 — 长行裁剪

**成立性：✅ 成立。** [search_inline:824](../../../crates/agentdash-application/src/vfs/relay_service.rs#L824)
只 `line.trim()`。

**收益：中。** minified/base64 是真实场景但不是高频。

**成本：低。** 在 service 层（inline）/ provider 层（canvas/skill_asset）
加 `MAX_LINE_LEN = 500` 截断 + 后缀 `...(truncated)`。≈ 30 LOC。

**决策：accept。** 落 `fs-tools-noise-control` 包。

---

### P1 #5 — VCS 默认黑名单

**成立性：✅ 成立。** 但**强依赖 F1 修复**——目前 include_glob 在非 inline
路径上被丢弃，加默认黑名单也无效。

**收益：高（在真实工程目录上）/ 低（在 canvas/inline 上，因为内容是 DB 数据，
不会有 .git 目录）。**

**成本：低。** F1 修复后，`fs_grep` tool 层把
`!**/.git`、`!**/.svn` 等 6 个 pattern 拼到 `include_glob` 前面。≈ 20 LOC。

**决策：accept。** 依赖 `vfs-search-contract-fix`。

落 `fs-tools-noise-control` 包。

---

### P1 #6 — case-insensitive / multiline / -A/-B

**成立性：✅ 成立。** 强依赖 F1 修复。

**收益：中。**

**成本：**

- F1 修复后，`SearchQuery.case_sensitive` 已存在；只需 `FsGrepParams` 加
  `case_insensitive: bool` 透传。
- `multiline`（`(?s)` 修饰）：regex crate 原生支持，inline 路径直接
  `regex::RegexBuilder::new(...).dot_matches_new_line(true)`；其他 provider
  各自实现。
- 独立 `-A/-B`：扩展 `TextSearchParams` 加 `before_lines/after_lines`，
  context_lines 仍用作"对称"快捷。
- 总计 ≈ 60 LOC。

**决策：accept。** 依赖 `vfs-search-contract-fix`。落 `fs-tools-noise-control` 包。

---

### P1 #7 — fs_glob truncated 上限 + mtime 排序

**成立性：✅ 成立。** [glob.rs:83-110](../../../crates/agentdash-application/src/vfs/tools/fs/glob.rs#L83-L110)
没有 max + 没有 sort。

**收益：高。** recursive=true 在真实仓库下噪音爆炸。

**成本：低。**

- `FsGlobParams` 加 `max_results: Option<usize>`，默认 100。
- 在 tool 层 sort：`entries.sort_by_key(|e| std::cmp::Reverse(e.modified_at.unwrap_or(0)))`，
  modified_at 缺失的统一沉底。
- 截断时尾部加 `(N more entries; refine pattern or raise max_results)`。
- 加 `truncated: bool` 字段输出。
- 总计 ≈ 50 LOC。

**决策：accept。** 落 `fs-tools-noise-control` 包。

---

## 3. P2 三条 — 应用 §0 对齐基线后重新评估

> **Updated（用户反馈）：** 原版评估按"是否本地最优"判断，结果两条 defer
> 一条 reject。应用"对齐 CC 基线"原则后，三条都需要重新评估——CC 有的能力
> 默认应该有，除非进白名单。

| 项 | 原决策 | 新决策 | 理由 |
|----|--------|--------|------|
| #8 fs_read ENOENT 友好提示 | defer | **accept** | CC 有，对齐基线要求做。mount 抽象的复杂度通过"仅在调用方 mount 内搜索 + 跨 mount 时不做"降级处理 |
| #9 fs_glob `[dir]/[file]` 前缀 | reject | **accept（去掉前缀，改为 trailing slash）** | 原决策是"前缀是显式信息，CC 没有不代表我们错"；按对齐基线，CC 用 trailing slash 表达目录已经足够，每行省 6 字节 + 输出与 CC 一致更利于 LLM 迁移 |
| #10 fs_grep `type` 快捷键 | defer | **accept（最小集 5–10 种语言）** | CC 有，对齐基线要求做。维护成本通过"只覆盖 js/ts/py/rust/go/java 等核心 5–10 种"控制，不追求 ripgrep 全集 |

> P0#1 的"按 range 读"分支也已升级为 accept（见 §1 P0#1 的 Updated 段）。

---

### 3.1 — P2 #8：fs_read ENOENT 友好提示（accept，落到 fs-read-rebuild）

**当前状态：**

[read.rs:80-107](../../../crates/agentdash-application/src/vfs/tools/fs/read.rs#L80-L107)
拼路径失败时直接用 `AgentToolError::ExecutionFailed(MountError::NotFound)` 透传，
错误格式形如 `not found: src/foo/bar.rs`。Claude Code 那边
[FileReadTool.ts:638-648](../../../references/claude-code/src/tools/FileReadTool/FileReadTool.ts#L638-L648)
会做：

- `findSimilarFile`：在父目录里找编辑距离最近的同名候选。
- `suggestPathUnderCwd`：把"在 cwd 下的同名文件"建议出来。
- macOS 截图 thin-space fallback（场景太窄，已 reject 不讨论）。

**为什么 accept（按对齐基线）：**

CC 有这个能力（findSimilarFile + suggestPathUnderCwd），按对齐基线要做。
mount 抽象带来的复杂度用以下方案降级处理，**不阻塞 accept**：

1. **不跨 mount 搜索**：仅在用户传入路径所属的 mount 内做 fuzzy 匹配。
   跨 mount 搜索成本太高且语义不清晰；用户拼错 mount_id 时返回"unknown
   mount: xxx, available: [main, secondary]"即可（已有信息）。
2. **canvas/inline 上的 fuzzy 复用 list 输出**：fuzzy 匹配本质是
   "list + 编辑距离排序"，list 已经是 provider 的常规接口，全表扫成本
   等同于一次 fs_glob——这在用户已经拼错路径的窄窗口内可以接受。
3. **path 强约束的好处仍在**：mount 协议过滤掉了大量"路径乱猜"的失败模式，
   但**剩下的 5%**（同 mount 内拼错文件名）正是 ENOENT 友好提示的最大价值
   场景。

**实施方案：**

- 在 `MountProvider` SPI 加 `suggest_paths(prefix: &str, limit: usize)
  -> Vec<String>` 默认实现遍历 list 并按 levenshtein 排序。
- `FsReadTool` 在 `MountError::NotFound` 时调一次 `suggest_paths`
  取 top-3，拼进错误消息。
- LOC 估算：SPI 加默认实现 + 4 provider 复用默认实现 ≈ 80 LOC；工具层 ≈ 20 LOC。
- **建议工时：< 1 day。**

**落到 child task：** `fs-read-rebuild`（与 P0#1 + P0#2 合并）。

---

### 3.2 — P2 #10：fs_grep `type` 快捷键（accept，落到 fs-grep-rebuild）

**当前状态：**

[grep.rs:48-49](../../../crates/agentdash-application/src/vfs/tools/fs/grep.rs#L48-L49)
有 `include: Option<String>` 字段（语义为 glob pattern）。Claude Code 在 glob
之外多了个 `type: js | py | rust | ...`
([GrepTool.ts:74-79](../../../references/claude-code/src/tools/GrepTool/GrepTool.ts#L74-L79))，
直接走 `rg --type` 利用 ripgrep 内置的 ext 列表。

**为什么 accept（按对齐基线）：**

CC 有 `type: js | py | rust | ...`，按对齐基线要做。原版评估担心的
"维护映射表"问题用以下方案控制：

1. **只覆盖核心 5–10 种语言**（js/ts/py/rust/go/java/c/cpp/cs/rb），不追求
   ripgrep 全集。这些扩展名年级别稳定，维护成本低。
2. **映射表来自 ripgrep 源码的标准定义**（参考 ripgrep 的
   `--type-list` 输出），不自行发明，与 LLM 训练数据中的语义对齐。
3. **`type` 与 `glob` 共存**，type 是 ergonomic 快捷键，glob 是底层兜底；
   非 5–10 种语言用 glob 解决，不强求覆盖全。
4. **每次 grep 调用省 5-15 token 看似小**，但这是高频工具，累加效应可观。

**实施方案：**

- 在 `fs_grep` tool 层加 `type: Option<String>` 字段。
- 维护 `LANG_EXTENSIONS: &[(&str, &[&str])]` 静态映射表（≈ 30 行）。
- 在 execute 中把 `type` 翻译为 glob 模式（与现有 glob 字段叠加：取并集）。
- LOC 估算：≈ 50 LOC + 单元测试。
- **建议工时：< 0.5 day。**

**落到 child task：** `fs-grep-rebuild`。

---

### 3.3 — P0#1 的 range 读分支（升级为 accept）

**为什么升级（用户反馈）：**

1. **range 读是 fs_read × fs_grep 的标准协作链路**：grep 拿到 `path:line:content`
   后，read 用 offset/limit 精读上下文是高频用法。CC 这条链路本身就是支持的，
   我们若不做 range 读会让这条链路出现"草率 slice"的隐性性能坑。
2. **对齐基线（§0）要求**：`offset/limit` 在 CC 是真按 range 读的，不允许
   "全文加载后 slice"。
3. **canvas/inline 看似收益为零，但接入统一 SPI 后行为可预测**：避免
   "看起来支持 range 实际没省内存"的隐性陷阱；统一 SPI 也让后续如果改
   in-DB 存储为外部文件时不需要再改工具层。

**实施方案：**

- SPI 扩展：`MountProvider::read_text_range(mount, path, offset, limit, ctx)
  -> Result<ReadResult, MountError>`，默认实现为 `read_text` + slice（兼容现状）。
- lifecycle / relay_fs：重写为 `BufRead::lines().skip(offset).take(limit)`
  或字节定位 + line counter，避免全文加载。
- canvas / inline / skill_asset：保留默认实现（in-DB 数据已在内存，slice
  即可），不重写。
- 工具层：`FsReadTool::execute` 改用 `read_text_range`。
- LOC 估算：SPI 默认实现 ≈ 30 LOC；工具层接入 ≈ 30 LOC；lifecycle 重写
  ≈ 60 LOC；relay_fs 重写 ≈ 60 LOC。
- **建议工时：1–3 day。**

**落到 child task：** `fs-read-rebuild`（与 P0#1 上限 + P0#2 dedup + P2#8
友好提示一起做，构成一个完整的"fs_read 重做"任务）。

---

### 3.4 — accept 后的优先级序

> 原版的"defer 重启信号表"已不再适用——三项都升级为 accept。
> 改为列出所有 child task 内部的实施优先级建议（见 §4 决策矩阵的工时栏）。


---

## 4. 决策矩阵 & Child Task 拆分

> **Updated（用户反馈）：** 应用对齐基线后所有 P2 项升级为 accept，
> 拆分逻辑也从"按改造维度"改为"按工具 rebuild"——每个工具一个 child task，
> 内部把所有相关的 P0/P1/P2 项 + schema 对齐 + 行为对齐一并完成，避免
> "改了一半"的中间态。

### 决策矩阵

| ID | 名称 | 决策 | 工时 | 依赖 | 落到 child |
|----|------|------|------|------|-----------|
| F1 | SearchQuery 字段补齐 + SearchResult.truncated 透传 | accept | 1–3 day | — | `vfs-search-spi-fix` |
| F2 | ReadResult 加 version_token（dedup 用） + modified_at（mtime 用） | accept | < 1 day | — | `vfs-search-spi-fix` |
| F4 | SPI 加 read_text_range（默认 read_text + slice，允许重写） | accept | < 1 day | — | `vfs-search-spi-fix` |
| P0#1 | fs_read 上限 + 真按 range 读 + schema 对齐 CC（offset/limit） | accept | 1–3 day | F1+F2+F4 | `fs-read-rebuild` |
| P0#2 | fs_read dedup（基于 version_token） | accept | < 1 day | F2 | `fs-read-rebuild` |
| P0#3 | fs_grep output_mode + schema 对齐 CC（pattern/glob/head_limit） | accept | < 1 day | — | `fs-grep-rebuild` |
| P1#4 | fs_grep 长行裁剪（max_columns 500） | accept | < 1 day | — | `fs-grep-rebuild` |
| P1#5 | fs_grep VCS 默认黑名单（6 个目录） | accept | < 1 day | F1 | `fs-grep-rebuild` |
| P1#6 | fs_grep -i / multiline / -A/-B/-C / -n | accept | 1–3 day | F1 | `fs-grep-rebuild` |
| P1#7 | fs_glob truncated + mtime 排序 + 默认上限 100 | accept | < 1 day | F2 | `fs-glob-rebuild` |
| P2#8 | fs_read ENOENT 友好提示（同 mount 内 fuzzy） | accept | < 1 day | — | `fs-read-rebuild` |
| P2#9 | fs_glob 去 `[dir]/[file]` 前缀，改用 trailing slash | accept | < 1 day | — | `fs-glob-rebuild` |
| P2#10 | fs_grep `type` 快捷键（5–10 种语言映射表） | accept | < 1 day | — | `fs-grep-rebuild` |
| —— | fs_glob schema 对齐（去 recursive、pattern 必填、去 substring 退化） | accept | < 1 day | — | `fs-glob-rebuild` |
| —— | fs_grep query→pattern + 去 regex 字段（pattern 始终是正则） | accept | < 1 day | — | `fs-grep-rebuild` |
| FU#1 | SPI `SearchQuery` / `GrepQuery` trait split | follow-up | 1–3 day | 全部 4 child 完成 | `vfs-grep-query-split`（独立 follow-up task，本评估任务不开） |

### Child Task 建议（4 个 — 按工具 rebuild）

> **Status：** 5 个 child task 已经创建并 link 到本 parent，PRD 已落地。
> 列表：
> - [05-25-vfs-search-spi-fix](../05-25-vfs-search-spi-fix/prd.md)（P0）
> - [05-25-fs-read-rebuild](../05-25-fs-read-rebuild/prd.md)（P0）
> - [05-25-fs-grep-rebuild](../05-25-fs-grep-rebuild/prd.md)（P0）
> - [05-25-fs-glob-rebuild](../05-25-fs-glob-rebuild/prd.md)（P1）
> - [05-25-vfs-grep-query-split](../05-25-vfs-grep-query-split/prd.md)（P2 follow-up）

1. **`vfs-search-spi-fix`**（必须最先做）
   - **范围（SPI 层）：**
     - `SearchQuery` 加 `is_regex / include_glob / context_lines / before_lines /
       after_lines / case_insensitive / multiline / output_mode`。
     - `SearchResult` 加 `truncated: bool`。
     - `ReadResult` 加 `version_token: Option<String>` + `modified_at: Option<i64>`。
     - `MountProvider` 加 `read_text_range(mount, path, offset, limit, ctx)
       -> Result<ReadResult, MountError>`，默认实现为 `read_text` + slice。
     - `MountProvider` 加 `suggest_paths(mount, prefix, limit, ctx)
       -> Result<Vec<String>, MountError>`，默认实现遍历 list + levenshtein 排序。
     - 修复 `relay_service::search_text_extended` 把所有新字段透传给 provider。
     - 4 个 provider 至少在签名上接入新字段（默认行为不变）。
   - **工时：** 1–3 day。
   - **验收：** 现有调用者无需改动；新加集成测试覆盖 inline / canvas /
     lifecycle 三个 provider；F1/F2/F3/F4 都解决。

2. **`fs-read-rebuild`**
   - **范围：**
     - schema 对齐 CC：`start_line/end_line` → `offset/limit`。
     - 真按 range 读：调用 `read_text_range`。
     - 字节 + 行数双阈值上限 + 拒绝消息。
     - dedup（基于 version_token，LRU 64，per-session）。
     - ENOENT 友好提示（调用 `suggest_paths`，同 mount 内）。
     - lifecycle / relay_fs 的 `read_text_range` 优化实现（避免全文加载）。
     - prompt 描述更新对齐 CC。
   - **工时：** 1–3 day。
   - **依赖：** `vfs-search-spi-fix` 已 merge。

3. **`fs-grep-rebuild`**
   - **范围：**
     - schema 对齐 CC：`query` → `pattern`；去除 `regex` 字段（pattern 始终
       是正则）；`include` → `glob`；`max_results` → `head_limit`（默认 250、
       `0` = 无限）；加 `offset`。
     - 加 `output_mode: content | files_with_matches | count`（默认
       files_with_matches）。
     - 加 `-i` / `multiline` / `-A` / `-B` / `-C` / `context` / `-n`。
     - 长行裁剪（max_columns 500）。
     - VCS 默认黑名单（6 个目录）。
     - 加 `type: Option<String>`（5–10 种语言映射表）。
     - prompt 描述更新对齐 CC。
   - **工时：** 1–3 day。
   - **依赖：** `vfs-search-spi-fix` 已 merge。

4. **`fs-glob-rebuild`**
   - **范围：**
     - schema 对齐 CC：去 `recursive` 字段；`pattern` 改必填；去 substring
       退化语义（统一 glob）。
     - 加默认 `max_results: 100` + `truncated: bool` 输出。
     - mtime desc 排序（缺失 fallback path）。
     - 输出格式对齐 CC：去 `[dir] / [file]` 前缀，目录用 trailing slash。
     - prompt 描述更新对齐 CC。
   - **工时：** < 1 day。
   - **依赖：** `vfs-search-spi-fix` 已 merge（用 modified_at）。

### 拆分逻辑

- **`vfs-search-spi-fix` 是基础设施层 fix**，必须先做，所有工具 rebuild 都依赖它。
- **三个 rebuild 任务按工具拆**，因为对齐 CC 的 schema 改动（参数命名）必须
  和功能改造一起做——分散做会出现"参数已改但行为没变"或反之的中间态，
  对调用方语义不友好。
- **三个 rebuild 任务可以并行**（彼此不耦合，只依赖 #1）。

### 破坏性变更说明 + 发布节奏（用户决议）

**Breaking change 清单：**

- `fs_read.start_line/end_line` → `offset/limit`
- `fs_grep.query` → `pattern`，去除 `regex` 字段（pattern **始终**是正则；
  literal 查询要求 LLM 自己 escape，与 CC 完全对齐）
- `fs_grep.include` → `glob`
- `fs_grep.max_results` → `head_limit`
- `fs_glob.recursive` 去除
- `fs_glob.pattern` 改为必填，去 substring 退化语义
- `fs_glob` 输出去 `[dir]/[file]` 前缀，改 trailing slash

**发布节奏（用户决议 A6）：四个 child task 全部同 release，中间不 merge 到 main。**

具体路径：

1. 开 `release/fs-tools-rebuild` 长期分支（基于 main）。
2. `vfs-search-spi-fix` 在该分支上做（不直接进 main）。
3. 三个 rebuild 在该分支上并行做（基于 spi-fix 的中间提交）。
4. agent 端 prompt + tool schema 更新一并打到该分支。
5. 全部就绪后整 branch 合并到 main，发一个 release tag。

**理由（用户原话之外的延伸）：** 避免"SPI 修了但工具还是老的"的中间态——
那种状态下 main 上 `search_text_extended` 接受新字段但所有 fs_* 工具都不传，
集成测试会出现假成功（默认行为不变意味着没有覆盖到新代码路径）。

**例外项：** 如果 `vfs-search-spi-fix` 的实现规模意外膨胀，可考虑分两个
release 走（spi-fix 先走、3 rebuild 后走），但**不允许**只发布部分 rebuild
（schema 不一致会让调用方混乱）。

**regex 字段的 breaking 处理（用户决议 A7）：直接 breaking，不留兼容期开关。**
prompt 里明确 "pattern is a regular expression"，与 CC 完全一致。LLM 在项目
间迁移时形成统一心智模型。

---

## 5. Open Questions — 决议状态

> **Updated（brainstorm 第二轮）：** 全部 8 个 question 中 5 个已决议。

### 已决议

- **~~A1~~ dedup 缓存大小** = **LRU 64 entries**（采纳建议默认值）。
- **~~A2~~ fs_read 字节/行数双阈值** = **`MAX_BYTES = 256KB`，`MAX_LINES = 5000`**
  （采纳建议默认值；超限抛错并提示用 `offset/limit`）。
- **~~A4~~ version_token 取不到时的 fallback** = 取不到则**不命中 dedup**
  （不引入常量 token，避免误命中）。
- **~~A5~~ Child task 拆分** = 4 个（1 SPI fix + 3 rebuild），按工具拆。
- **~~A6~~ breaking change 发布节奏** = 四个 child task **全部同 release**，
  中间不 merge 到 main。详见 §4 "破坏性变更说明 + 发布节奏"。
- **~~A7~~ fs_grep regex 字段** = **直接 breaking**（pattern 始终是正则，与 CC
  完全对齐）。

### 仍开放（不阻塞本评估任务，留到 child task brainstorm）

- **A3.** VCS 黑名单：做成可配置（mount metadata 里 `vfs_search_excludes`
  字段）vs 硬编码？**建议硬编码**（6 个目录变化频率低，可配置带来 schema
  复杂度）。等 `fs-grep-rebuild` brainstorm 时确认。
- **A8.** `fs_glob` 去除 `recursive` 字段后，旧调用方（带 `recursive: true`）
  应该报错还是静默忽略？**建议报错**（更早暴露调用方需更新）。等
  `fs-glob-rebuild` brainstorm 时确认。

这两个剩余问题不影响本评估任务的完成，本任务可以收尾。

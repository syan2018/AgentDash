# fs_read / fs_grep / fs_glob 工具优化评估

## Goal

参考 `references/claude-code/src/tools/{GrepTool,GlobTool,FileReadTool}` 的实现，
对当前 `crates/agentdash-application/src/vfs/tools/fs/` 三个工具做一次落差评审，
产出可执行的优化路线图。**本任务的产物是评估结论 + 决策**，不直接落实施代码——
真正的改造按结论再拆为后续 child task。

## Background

工具实现位置：
- `crates/agentdash-application/src/vfs/tools/fs/grep.rs` (`fs_grep`)
- `crates/agentdash-application/src/vfs/tools/fs/glob.rs` (`fs_glob`)
- `crates/agentdash-application/src/vfs/tools/fs/read.rs` (`fs_read`)
- 底层：`crates/agentdash-application/src/vfs/relay_service.rs` 中的
  `search_text_extended` / `search_inline` / `list` / `read_text`。

参考实现位置：
- `references/claude-code/src/tools/GrepTool/GrepTool.ts`
- `references/claude-code/src/tools/GlobTool/GlobTool.ts`
- `references/claude-code/src/tools/FileReadTool/FileReadTool.ts`

会话内已经做过一轮初评，识别出的候选改造点（按优先级）：

### P0（高杠杆）

1. **fs_read 全文加载后再过滤**：`read.rs:108-119` 拿全文 `result.content` 后
   `.lines().enumerate().filter()`，任意行范围都会全量搬运。Claude Code 用
   `readFileInRange` 按字节区间读取 + `validateContentTokens` 卡 token 上限。
2. **fs_read 缺去重（dedup）**：Claude Code 维护 `readFileState`，命中"同 path
   + 同 offset/limit + mtime 未变"返回 `file_unchanged` 短桩，BQ 上 ~18% Read 命中。
   我们每次都全量重读 + 重格式化。
3. **fs_grep 缺 `output_mode`**：当前永远返回 `file:line: content`。Claude Code
   提供 `files_with_matches` / `content` / `count` 三档，**默认 files_with_matches**。
   "符号在哪用过"类探针只要文件名列表就够，目前是几十倍 token 浪费。

### P1（噪音 / 召回质量）

4. **长行不裁剪**：`search_inline` 只 `line.trim()`，命中 minified/base64 整行原样进 hits；
   Claude Code 默认 `--max-columns 500` 兜底。
5. **VCS 默认排除缺失**：Claude Code 写死 `.git/.svn/.hg/.bzr/.jj/.sl` 负向 glob；
   我们完全交给上层 `include_glob`。
6. **fs_grep 缺 case_insensitive / multiline / 独立 -A/-B**：现在只有对称 `context_lines`，
   regex crate 自带 `(?i)`/`(?s)` 修饰，扩展成本极低。
7. **fs_glob 没有显式 truncated 上限**：service 返回多少就 join 多少。Claude Code 默认
   max_results=100 + truncated 标记 + mtime desc 排序。

### P2（锦上添花）

8. fs_read 的 ENOENT 友好提示（did-you-mean / suggestPathUnderCwd）。
9. fs_glob 的 `[dir]/[file]` 前缀是否值得为每行多付 6 字节。
10. fs_grep 的 `type: js|py|rust` 快捷键（Claude Code 走 `rg --type`，我们可维护小映射表）。

### 不建议照搬

- PDF / notebook 解析（与 mount-based VFS 模型不对齐，引入 poppler 依赖）。
- macOS 截图 thin-space fallback（场景太窄）。
- SDK 侧 deny-rule 权限模型（我们 `MountCapability + identity` 表达力更强）。

## Requirements

### R1 — 二次核实评估结论

针对 P0/P1 的每一条，需要在评估阶段确认：

- 该差距在我们当前调用链路里是否**真的**存在？（不是看表面 API，而是看
  RelayVfsService → MountProvider 的真实行为，特别是不同 provider 间的差异：
  `provider_canvas` / `provider_inline` / `provider_lifecycle` /
  `provider_skill_asset` / 真实 fs provider）。
- 改造的**预估 token 收益**与**实施成本**（用 LOC + 跨 crate 影响范围估算）。
- 是否有破坏性影响（schema 变更、is_error 语义、tool prompt 变更）。

### R2 — 输出落地路线图

评估结束时给出一份决策矩阵：每条 P0/P1 候选项标注 `accept` / `reject` /
`defer`，accept 项说明：
- 落到哪个 child task。
- 估算工时区间（< 1 day / 1–3 day / > 3 day）。
- 依赖（是否依赖 agent 进程级缓存、是否需要 service 层先重构等）。

### R3 — 评估**不**包含的事情

- 不写实现代码。
- 不修改任何 `crates/**/*.rs`。
- 不动 prompt 描述（描述变更交给 child task）。
- 不预先创建 child task（只在 PRD 里列出"建议拆分"）。

## Acceptance Criteria

- [ ] PRD 里列出的 P0/P1 每条候选项，在 design.md 中都有"是否成立 + 收益估算 +
      成本估算"三段式结论。
- [ ] design.md 末尾有一份决策矩阵（accept/reject/defer + 工时 + 依赖）。
- [ ] design.md 明确标注每条 accept 项**应该落到哪个 child task**（建议命名 + 范围）。
- [ ] implement.md 列出本评估任务的执行步骤（读哪些文件、跑哪些 grep、需要
      与用户确认的开放问题）。
- [ ] 最终用户对决策矩阵 review pass，本任务即可 archive；child task 由后续单独开。

## Resolved Decisions

> 以下条目在 brainstorm 阶段已与用户对齐，design.md 不得反向推翻；若评估阶段
> 发现强证据要求改判，必须回到本节更新后再继续。

1. **评估范围 = 全量 P0 + P1（7 条）。** P2 三条仍只列入 follow-up 建议，不在
   本任务核实。
2. **去重缓存层级 = `FsReadTool` 实例字段（每 session 一份）。** 与 Claude
   Code `readFileState` 语义对齐，规避跨 session 隔离风险。design.md 仍需明确
   并发访问边界（同 session 内是否多 tool call 并行？需 Mutex 还是 RwLock？）。
3. **fs_read 大文件上限 = 字节阈值 + 行数阈值双触发。** 任一超限即拒绝并
   提示用 `start_line` / `end_line` 分段读。design.md 需要给出建议数值
   （字节默认值、行数默认值）+ 是否允许 caller override。
4. **fs_grep `output_mode` 加在哪一层 = 评估阶段不定，design.md 必须列出
   "Tool 层去重" vs "Service 层原生支持" 两个方案的对比表**（实现成本、
   provider 改动范围、性能差异、最终采纳建议）+ 决策由用户在 review 时拍板。

## Remaining Open Questions

> 下列问题留到 design.md 阶段补齐答案；不阻塞本 PRD 的定稿。

- A1. dedup 缓存的容量与失效策略（LRU 大小？mtime 变更立即驱逐？）
- A2. 字节/行数双阈值的具体数值。
- A3. P1 的 VCS 默认黑名单：硬编码在 service 层 vs 走配置文件。
- A4. P1 的 fs_glob `truncated + mtime desc 排序`：mtime 在哪些 provider
  上不可得（如 inline / canvas）？不可得时的回退顺序？
- A5. Child task 的拆分粒度：PRD 初稿建议 3 个（fs-read-range-and-dedup /
  fs-grep-output-modes / fs-tools-noise-control），是否合理？

## Notes

- 评估阶段允许 read 任意 `crates/`、`references/`、`packages/` 文件。
- 评估产物以本任务的 design.md 为权威；初评结论可推翻。
- 后续 child task 命名建议：`fs-read-range-and-dedup`（P0 #1+#2）、
  `fs-grep-output-modes`（P0 #3）、`fs-tools-noise-control`（P1 #4-#7 合并包）。

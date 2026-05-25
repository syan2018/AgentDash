# Implement — fs_read / fs_grep / fs_glob 工具优化评估

> 本任务的产物是 [design.md](./design.md) 的决策结论 + child task 建议，
> **不写实现代码**。本文是评估任务自己的执行步骤记录。

## 0. 任务定位

- 类型：Planning / 评估任务（不修改 `crates/**/*.rs`）。
- 输入：[prd.md](./prd.md) + 4 份 Claude Code 工具源码（references/claude-code）。
- 输出：[design.md](./design.md)（决策矩阵 + child task 建议）。
- 验收：用户 review design.md 通过后即可 archive。

## 1. 已完成步骤

- [x] **S1 — Brainstorm 决议**：与用户对齐评估范围、dedup 层级、token 阈值
      模式、output_mode 决策路径。结论已写入 PRD `Resolved Decisions` 段。
- [x] **S2 — 三个工具源码核实**：通读
      [fs/grep.rs](../../../crates/agentdash-application/src/vfs/tools/fs/grep.rs)
      / [fs/glob.rs](../../../crates/agentdash-application/src/vfs/tools/fs/glob.rs)
      / [fs/read.rs](../../../crates/agentdash-application/src/vfs/tools/fs/read.rs)
      与 Claude Code 三份对应实现。
- [x] **S3 — service 层核实**：读
      [relay_service.rs](../../../crates/agentdash-application/src/vfs/relay_service.rs)
      关键函数 `search_text_extended` / `search_inline`，发现 F1（SearchQuery
      字段不全）、F3（truncated 永远 false）。
- [x] **S4 — provider 抽样核实**：读 4 个 provider 的 read_text/list/search_text
      接口形态，发现 F2（ReadResult 无 mtime）、F4（read_text 整文件语义）。
- [x] **S5 — agent 进程内现状核实**：grep `readFileState/dedup/file_state` 全 crates，
      确认 F5（无现成等价物）。
- [x] **S6 — design.md 一稿**：每条候选项三段式 + 决策矩阵 + child task 拆分
      （4 个）+ 5 个 open question 留给后续 child task。
- [x] **S7 — 用户偏好对齐**（重大方向调整）：用户给出"fs builtin 工具与
      Claude Code 整体对齐（包括参数）"的项目级原则。
      已写入 [feedback_fs_tools_align_with_claude_code](../../../../../.claude/projects/d--ABCTools-Dev-AgentDashboard/memory/feedback_fs_tools_align_with_claude_code.md)
      memory，并刷新 design.md：
      - 新增 §0 对齐 diff 表（三个工具的 schema/语义 vs CC）。
      - P0#1 升级：range 读从 defer → accept，与上限一并做。
      - P2 三项全部升级 accept（#9 从 reject 升级为 accept，去前缀改 trailing slash）。
      - child task 拆分从"按改造维度"改为"按工具 rebuild"：
        1 SPI fix + 3 rebuild（fs-read / fs-grep / fs-glob 各一）。
      - 新增 §4 末尾"破坏性变更说明"：四个 child task 必须同 release 发布。
      - Open Questions 刷新（A4/A5 隐含解决，新增 A6-A8 关于 breaking 处理）。
- [x] **S8 — Brainstorm 第二轮决议**：与用户对齐 4 个剩余决策点。
      - **命名分层**：tool 用 grep/glob/read（已对齐 CC），SPI 保持
        search/list/read（语义本质）；方向上拆 `GrepQuery extends SearchQuery`
        作为 follow-up `vfs-grep-query-split` task，本评估任务范围内不动。
      - **breaking 发布节奏（A6）**：四个 child task **全部同 release**，
        开 `release/fs-tools-rebuild` 长期分支，中间不 merge 到 main。
      - **regex 字段（A7）**：**直接 breaking**，pattern 始终是正则，prompt
        写明"is a regular expression"。
      - **A1/A2 数值**：**采纳建议默认值**（LRU 64 / MAX_BYTES 256KB /
        MAX_LINES 5000）。
      - design.md §0 加"命名分层"段；§4 决策矩阵加 FU#1 follow-up 行；§4
        破坏性变更说明改写为"全部同 release + release branch 路径"；§5 Open
        Questions 标记 5 项已决议（剩 A3/A8 留给 child task brainstorm）。
- [x] **S9 — 创建 5 个 child task 并落 PRD**：
      - 用 `task.py create --parent` 建立父子关系。
      - [05-25-vfs-search-spi-fix](../05-25-vfs-search-spi-fix/prd.md)（P0，
        SPI 层基础设施修复，必须最先做）
      - [05-25-fs-read-rebuild](../05-25-fs-read-rebuild/prd.md)（P0，
        schema 对齐 + range 读 + 上限 + dedup + ENOENT 友好提示）
      - [05-25-fs-grep-rebuild](../05-25-fs-grep-rebuild/prd.md)（P0，
        schema 对齐 + output_mode + 全部 grep 开关 + VCS 黑名单 + 长行裁剪 +
        type 快捷键）
      - [05-25-fs-glob-rebuild](../05-25-fs-glob-rebuild/prd.md)（P1，
        schema 对齐 + 默认上限 + mtime 排序 + trailing slash 输出）
      - [05-25-vfs-grep-query-split](../05-25-vfs-grep-query-split/prd.md)
        （P2 follow-up，trait split；不与 4 个 rebuild 同期，等 release branch
        merge 后再开工）
      - 每个 PRD 都列了：goal / requirements / acceptance criteria /
        constraints / 依赖 / 在 release branch 上完成不直接 merge 到 main。

## 2. 评估范围之外

以下事项**不在本任务覆盖**，明确转 child task 或 follow-up：

- 任何 `crates/**/*.rs` 的代码改动 → child task。
- 任何 prompt 描述更新 → child task 内顺带做。
- 实际的 SPI 兼容性测试 / provider 集成测试 → `vfs-search-contract-fix`。
- benchmark / profiler 数据收集（确认 P0#1 range 读 是否值得做）→ follow-up。

## 3. 执行风险与降级

| 风险 | 触发条件 | 降级路径 |
|------|---------|---------|
| design.md 决策被 review 推翻 | 用户对方案 D（version_token） / 4 个 child 拆分 不满意 | 回到 PRD `Resolved Decisions` 段更新，重写 design.md 受影响章节 |
| 核实存在遗漏（找到第 5 个 provider） | grep 漏掉 mod.rs 之外的 provider | 在 design.md 顶部加 F7 补丁段，更新决策矩阵 |
| Open Question A1-A5 中某项需要先答 | 某条 child task 的 PRD 需要明确数值才能写 | 在该 child task 的 brainstorm 阶段单独问；本任务不阻塞 |

## 4. 完成判定

- [x] design.md §0 有"对齐 CC 基线"diff 表（fs_read / fs_grep / fs_glob 三张）。
- [x] design.md 中 P0#1 / P0#2 / P0#3 / P1#4-#7 / P2#8-#10 十条都有评估段。
- [x] design.md 末尾有决策矩阵（≥ 13 行：F1+F2+F4 SPI 修复 + 7 条 P0/P1 + 3 条 P2 + schema 对齐 + FU#1）。
- [x] design.md 给出 4 个 child task 建议名（1 SPI fix + 3 rebuild）+ 范围 + 工时区间 + 依赖关系。
- [x] design.md 末尾有破坏性变更说明，明确四个 child task 必须同 release 发布。
- [x] **5 个 child task 已创建并 link 到 parent，PRD 全部落地**：
  - [05-25-vfs-search-spi-fix](../05-25-vfs-search-spi-fix/prd.md)
  - [05-25-fs-read-rebuild](../05-25-fs-read-rebuild/prd.md)
  - [05-25-fs-grep-rebuild](../05-25-fs-grep-rebuild/prd.md)
  - [05-25-fs-glob-rebuild](../05-25-fs-glob-rebuild/prd.md)
  - [05-25-vfs-grep-query-split](../05-25-vfs-grep-query-split/prd.md)（follow-up）
- [x] 用户 review parent design.md + 5 个 child PRD 通过。
- [ ] **5 个 child task 全部 archive**（parent task 收尾条件，**不**提前
      archive parent）。

> **Parent archive 策略**（用户决议 + memory 沉淀）：
> 父任务在所有 child archive 前**保持 planning 状态**。理由：保留"child
> 在 execute 阶段发现 parent 决策需修订时回流到 parent design.md 更新"
> 的反馈回路。本任务不走 `task.py start`、不走 archive，留作 design 决策
> 的权威 reference，等 5 个 child 全部完成后统一 archive。
> 见 memory: [feedback_parent_task_no_early_archive](../../../../../.claude/projects/d--ABCTools-Dev-AgentDashboard/memory/feedback_parent_task_no_early_archive.md)。

## 5. Review checklist 给用户

请在 review 时重点检查：

1. **§0 对齐 diff 表是否抓全？** 三个工具的参数/输出/语义对照表是否还有
   遗漏的偏离项？故意偏离白名单（mount 协议保留、不做 PDF/notebook）是否够？
2. **F1/F2/F4 三个 SPI 修改打包到 `vfs-search-spi-fix`** 是否合理？
   还是要把 ReadResult 扩展拆出成独立任务？
3. **按工具 rebuild 的拆分** 是否合你预期？相比按改造维度拆，rebuild 拆法
   每个 child task 内部包含 schema 改名 + 功能改造 + prompt 更新，
   一致性更高但单任务工时偏大（1–3 day）。
4. **breaking change 同 release 发布** 是否可行？需要协调 agent 端
   prompt + tool schema 同步更新（见 design.md A6-A8）。
5. **8 个 open question**（A1–A3, A6–A8）你想现在就回答还是延到 child task
   brainstorm 阶段？A6-A8 关于 breaking change 处理的回答会影响发布节奏。

**Parent task 不走 `task.py start`/不 archive**：本任务的产物是 design 决策
矩阵 + 5 个 child task 的 PRD，在 5 个 child 全部完成前保持 planning，
留作权威 reference。后续按 child 各自的流程推进。

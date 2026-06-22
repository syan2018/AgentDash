# Research: reference-claude-code-large-output

- Query: 研究 `references/claude-code` 中与大工具输出、tool result truncation、transcript 存储、本地 artifact、resume 恢复、terminal 输出保留相关的实现和文档实践。
- Scope: internal
- Date: 2026-06-22

## Findings

### Files found

- `references/claude-code/README.md` - 说明该目录是 2026-03-31 公开暴露 source map 后的 Claude Code TypeScript/Bun/Ink 源码快照，非官方仓库。
- `references/claude-code/src/constants/toolLimits.ts` - tool result 全局阈值：`DEFAULT_MAX_RESULT_SIZE_CHARS = 50_000`、`MAX_TOOL_RESULT_BYTES = 400_000`、`MAX_TOOL_RESULTS_PER_MESSAGE_CHARS = 200_000`。
- `references/claude-code/src/utils/toolResultStorage.ts` - 通用大 tool result 落盘、preview、模型可见替换、per-message aggregate budget、resume replacement state。
- `references/claude-code/src/tools/BashTool/BashTool.tsx` - Bash 输出 inline 上限、超大输出复制/硬链接到 `tool-results`、模型可见 `<persisted-output>` 说明。
- `references/claude-code/src/utils/shell/outputLimits.ts` - Bash inline 输出上限配置：`BASH_MAX_OUTPUT_DEFAULT = 30_000`，`BASH_MAX_OUTPUT_UPPER_LIMIT = 150_000`。
- `references/claude-code/src/utils/Shell.ts` / `src/utils/ShellCommand.ts` - shell stdout/stderr 直接写文件 FD、后台任务 size watchdog、foreground 只读有界输出。
- `references/claude-code/src/utils/task/TaskOutput.ts` / `src/utils/task/diskOutput.ts` - background/task 输出文件、tail/range 读取、5GB disk cap、8MB 默认 range/tail read。
- `references/claude-code/src/tools/TaskOutputTool/TaskOutputTool.tsx` / `src/utils/task/outputFormatting.ts` - deprecated task output 读取工具，API 输出再按 `TASK_MAX_OUTPUT_LENGTH` 截尾并提示完整文件路径。
- `references/claude-code/src/services/mcp/client.ts` / `src/utils/mcpValidation.ts` / `src/utils/mcpOutputStorage.ts` - MCP 大输出 token 判定、文本落盘、二进制 blob 落盘、失败回退。
- `references/claude-code/src/utils/sessionStorage.ts` / `src/types/logs.ts` / `src/utils/sessionStoragePortable.ts` / `src/utils/sessionRestore.ts` / `src/screens/REPL.tsx` - JSONL transcript、`content-replacement` 记录、resume/reconstruct、lite metadata/head-tail 读取、large transcript 加载。
- `references/claude-code/src/services/tools/toolExecution.ts` / `src/utils/messages.ts` - tool execution 生成 user `tool_result` 消息，并同时保留 native `toolUseResult`。
- `references/claude-code/src/utils/cleanup.ts` - session JSONL、`.cast`、`tool-results` 文件的按 mtime 清理。
- `references/claude-code/src/utils/transcriptSearch.ts` - UI 搜索刻意不索引模型专用 `<persisted-output>` wrapper，改搜 native `toolUseResult` 可见字段。

### Model-facing tool result offload

Claude Code 的通用路径把“模型可见 tool_result content”作为主要保护对象。

- `DEFAULT_MAX_RESULT_SIZE_CHARS = 50_000` 是默认每个工具结果落盘阈值；`MAX_TOOL_RESULTS_PER_MESSAGE_CHARS = 200_000` 防止并发工具结果合并成一个超大 user message；`MAX_TOOL_RESULT_BYTES = 400_000` 是按 100k tokens 估算出的兜底字节阈值。见 `references/claude-code/src/constants/toolLimits.ts:5`, `:12`, `:28`。
- `getPersistenceThreshold(toolName, declaredMaxResultSizeChars)` 使用工具声明值和全局默认值的较小者，允许 GrowthBook flag `tengu_satin_quoll` 覆盖；`maxResultSizeChars = Infinity` 的工具会跳过该机制，因为 Read 自己有 maxTokens。见 `references/claude-code/src/utils/toolResultStorage.ts:43`, `:55`, `:59`, `:77`。
- `persistToolResult(content, toolUseId)` 写入 `{projectDir}/{sessionId}/tool-results/{toolUseId}.txt|json`，使用 `writeFile(..., flag: 'wx')`，同一 `tool_use_id` 重放时遇到 `EEXIST` 直接复用，不重写。见 `references/claude-code/src/utils/toolResultStorage.ts:95`, `:104`, `:114`, `:137`, `:153`, `:157`, `:161`。
- 模型收到的替代内容由 `buildLargeToolResultMessage()` 生成，格式为 `<persisted-output>`，包含原始大小、完整文件路径、`Preview (first 2KB)`、以及 `...` 标记。`PREVIEW_SIZE_BYTES = 2000`，preview 优先在接近上限的换行处截断。见 `references/claude-code/src/utils/toolResultStorage.ts:108`, `:189`, `:192`, `:193`, `:194`, `:339`, `:347`。
- `maybePersistLargeToolResult()` 在内容为空时替换为短 marker，非空且非图片内容超阈值时落盘；落盘失败时直接返回原 block，不做强制安全拒绝。见 `references/claude-code/src/utils/toolResultStorage.ts:272`, `:287`, `:301`, `:306`, `:314`, `:316`。
- `applyToolResultBudget()` 在 query 前执行 aggregate budget；如果同一 API-level user message 中多个 tool_result 合计超限，挑最大的新鲜结果落盘替换，直到预算内。见 `references/claude-code/src/query.ts:370`, `:379`, `references/claude-code/src/utils/toolResultStorage.ts:740`, `:769`, `:828`。

### Per-message replacement state and resume

为了 prompt cache 稳定，Claude Code 不在每轮重新判断历史 tool result，而是记录每个 `tool_use_id` 的替换命运。

- `ContentReplacementState` 包含 `seenIds` 和 `replacements`。一旦见过某 tool result，替换/不替换命运冻结；已替换内容在后续请求中用 Map 直接重放，避免文件 I/O 和模板漂移。见 `references/claude-code/src/utils/toolResultStorage.ts:372`, `:375`, `:377`, `:802`。
- `ContentReplacementRecord` 是可序列化记录：`kind: 'tool-result'`、`toolUseId`、`replacement`；注释明确它会写入 transcript 以支持 resume。见 `references/claude-code/src/utils/toolResultStorage.ts:466`, `:475`。
- `ContentReplacementEntry` 是 JSONL entry，字段为 `type: 'content-replacement'`、`sessionId`、可选 `agentId`、`replacements`。见 `references/claude-code/src/types/logs.ts:175`, `:181`。
- `query.ts` 只对 `agent:*` 和 `repl_main_thread*` 调用 `recordContentReplacement()`，临时 forked agent 不写 replacement 记录。见 `references/claude-code/src/query.ts:373`, `:376`, `:383`。
- `sessionStorage.insertContentReplacement()` 把 main-thread replacement 写到 session JSONL，把 subagent replacement 写到 sidechain transcript。见 `references/claude-code/src/utils/sessionStorage.ts:1113`, `:1118`, `:1200`, `:1204`。
- `loadTranscriptFile()` 读取到 `content-replacement` entry 后按 `sessionId` 或 `agentId` 聚合，`LogOption.contentReplacements` 再供 resume 使用。见 `references/claude-code/src/utils/sessionStorage.ts:3682`, `:3690`, `:3915`, `:3921`, `:4689`。
- `provisionContentReplacementState(initialMessages, initialContentReplacements)` 在 REPL 初始 mount 时重建状态；in-session `/resume` 后用 `reconstructContentReplacementState(messages, log.contentReplacements)` 更新。见 `references/claude-code/src/screens/REPL.tsx:536`, `:1503`, `:1912`, `:1923`。
- `reconstructContentReplacementState()` 把 transcript 中所有 candidate id 标为 seen，只对有 record 的 id 填充 replacement；compact 后不在 messages 中的 record 被跳过。见 `references/claude-code/src/utils/toolResultStorage.ts:939`, `:946`, `:960`, `:972`, `:975`。
- `--fork-session` 特殊处理：新 session id 会拷贝原 replacement records，否则 fork 后的同一 tool_use_id 会被误判为 frozen 且发送 full content。见 `references/claude-code/src/utils/sessionRestore.ts:452`, `:455`, `:462`。

### Transcript persistence boundary

Claude Code 的 JSONL transcript 保存的是 `TranscriptMessage`，不只是模型 API payload。这里有一个重要边界：模型可见 `tool_result.content` 可能已被替换为 `<persisted-output>`，但消息对象还可能包含 native `toolUseResult`。

- `recordTranscript` 展开 `...message` 后写 JSONL，因此 user message 上的 `toolUseResult` 会随消息持久化。见 `references/claude-code/src/utils/sessionStorage.ts:1039`, `:1048`, `:1065`。
- `createUserMessage()` 明确定义 `toolUseResult?: unknown`，并把它放进 user message。见 `references/claude-code/src/utils/messages.ts:460`, `:481`, `:515`。
- `toolExecution.addToolResult()` 生成 `contentBlocks` 时使用经过 `processToolResultBlock()` 的模型 block，但 `toolUseResult` 字段保存原始 tool output；只有 subagent 且 `preserveToolUseResults` 为 false 时才置 `undefined`。见 `references/claude-code/src/services/tools/toolExecution.ts:1403`, `:1409`, `:1415`, `:1456`, `:1460`。
- 因此，Claude Code 对 transcript 的“完全不保存原文”并非统一 invariant。它依赖各工具先把 native output 自身有界化：Bash 的 `Out.stdout` 是有界 preview/path；MCP 的 `processMCPResult()` 直接返回 instruction/path；但通用工具若 native `Out` 很大，仍可能进入 JSONL 的 `toolUseResult`。
- `sessionStorage` 的大 transcript 处理更多是加载性能保护，不是内容去重：`MAX_TRANSCRIPT_READ_BYTES = 50MB`，lite metadata 只读 head/tail 64KB；超过 5MB 走 `readTranscriptForLoad()` 分块扫描 compact boundary。见 `references/claude-code/src/utils/sessionStorage.ts:229`, `references/claude-code/src/utils/sessionStoragePortable.ts:17`, `:209`, `:480`, `:717`。
- `sessionStorage` 的 chain scanner 注释也承认 `toolUseResult/mcpMeta` 是 server-controlled nested object，且会出现在 top-level uuid 之后。见 `references/claude-code/src/utils/sessionStorage.ts:3249`, `:3347`。
- UI 搜索规避了模型专用 wrapper：`transcriptSearch.ts` 不搜 tool_result block 的 `<persisted-output>`，而用 native `toolUseResult` 的 allowlist 字段。见 `references/claude-code/src/utils/transcriptSearch.ts:43`, `:49`, `:183`。

### Bash / terminal output lifecycle

Bash 是最接近 terminal 输出保留的完整实现。

- `BashTool.maxResultSizeChars = 30_000`，同时 `utils/shell/outputLimits.ts` 的默认 inline read 也是 30,000，上限 150,000，可由 `BASH_MAX_OUTPUT_LENGTH` 配置。见 `references/claude-code/src/tools/BashTool/BashTool.tsx:424`, `references/claude-code/src/utils/shell/outputLimits.ts:3`, `:6`。
- `TaskOutput` 是 shell 输出的 single source of truth。foreground/file mode 下 stdout 和 stderr 直接写同一个文件 FD，不进入 JS；progress 只 poll 文件 tail。见 `references/claude-code/src/utils/task/TaskOutput.ts:21`, `:24`, `references/claude-code/src/utils/Shell.ts:282`。
- `TaskOutput.#readStdoutFromFile()` 只从 output 文件头部读取 `getMaxOutputLength()` 字节；如果文件更大，`outputFileRedundant=false`，调用方会得到 `outputFilePath`/`outputFileSize`。见 `references/claude-code/src/utils/task/TaskOutput.ts:297`, `:298`, `references/claude-code/src/utils/ShellCommand.ts:297`, `:306`, `:312`。
- `BashTool` 对大输出把完整 output 文件复制/硬链接到 `{sessionId}/tool-results/{taskId}.txt`，如果文件大于 64MB 先 `truncate` 原文件到 64MB 再复制/链接；模型内容用 `buildLargeToolResultMessage()` 包装，UI 不显示该 wrapper。见 `references/claude-code/src/tools/BashTool/BashTool.tsx:728`, `:732`, `:739`, `:741`, `:745`, `:749`, `:589`。
- 这意味着 Bash 原文会落本地文件，但不是无限保留：模型可见的是前 2KB preview + path；`toolUseResult.stdout` 是头部 bounded 内容，`persistedOutputPath`/`persistedOutputSize` 是 metadata。见 `references/claude-code/src/tools/BashTool/BashTool.tsx:581`, `:591`, `:803`, `:814`。
- 后台 shell 仍用同一 `TaskOutput.taskId` 文件。`LocalShellTask` completion notification 只包含 `task_id`、`output_file`、`status`、`summary`，不塞完整输出；stall watchdog 只带 2KB tail。见 `references/claude-code/src/tasks/LocalShellTask/LocalShellTask.tsx:44`, `:60`, `:80`, `:158`, `:160`。
- `ShellCommand` 对 background file mode 启动 size watchdog，文件超过 `MAX_TASK_OUTPUT_BYTES_DISPLAY = 5GB` 时 kill；pipe mode 会 spill to disk。见 `references/claude-code/src/utils/ShellCommand.ts:52`, `:239`, `:246`, `:318`, `:349`, `:354`, `references/claude-code/src/utils/task/diskOutput.ts:25`, `:30`。
- `DiskTaskOutput.append()` 超过 5GB disk cap 后追加 `[output truncated: exceeded 5GB disk cap]` 并停止写入。见 `references/claude-code/src/utils/task/diskOutput.ts:110`, `:117`, `:120`。
- `TaskOutputTool` 已标 deprecated，提示更推荐直接用 Read 读 output file path；它读取当前/完成任务输出时仍通过 `getTaskOutput()` tail 8MB，再由 `formatTaskOutput()` 按 32,000 默认/160,000 上限截尾，并写 `[Truncated. Full output: path]` header。见 `references/claude-code/src/tools/TaskOutputTool/TaskOutputTool.tsx:144`, `:157`, `references/claude-code/src/utils/task/diskOutput.ts:336`, `references/claude-code/src/utils/task/outputFormatting.ts:3`, `:17`, `:26`。

### MCP large output and binary artifacts

MCP 大输出是单独的前置处理，不只依赖通用 `maxResultSizeChars`。

- `MCPTool.maxResultSizeChars = 100_000`，但实际 `processMCPResult()` 会先用 `mcpContentNeedsTruncation()` 判断 token 级输出是否超过 `MAX_MCP_OUTPUT_TOKENS`，默认 25,000 tokens。见 `references/claude-code/src/tools/MCPTool/MCPTool.ts:35`, `references/claude-code/src/utils/mcpValidation.ts:8`, `:13`, `:20`。
- `mcpContentNeedsTruncation()` 先用 rough estimate，超过阈值一半才调用 token count API；token count 失败时假定无需 truncation。见 `references/claude-code/src/utils/mcpValidation.ts:142`, `:148`, `:158`, `:161`。
- 如果 `ENABLE_MCP_LARGE_OUTPUT_FILES` 显式 false，或内容包含 image block，则回退 `truncateMcpContentIfNeeded()`，追加 `[OUTPUT TRUNCATED - exceeded ... token limit]` 和分页/过滤建议。见 `references/claude-code/src/services/mcp/client.ts:2740`, `:2756`, `references/claude-code/src/utils/mcpValidation.ts:72`, `:177`。
- 否则 MCP 大文本会通过 `persistToolResult(contentStr, persistId)` 落 `tool-results`，返回 `getLargeOutputInstructions()`，说明结果字符数、文件路径、格式、offset/limit/jq 使用要求。见 `references/claude-code/src/services/mcp/client.ts:2767`, `:2771`, `:2773`, `:2793`, `:2794`, `references/claude-code/src/utils/mcpOutputStorage.ts:39`, `:46`, `:48`。
- MCP binary/audio/blob 不把 base64 塞上下文，而是 `persistBinaryContent()` 以 MIME 推断扩展名写入 `tool-results`，模型只收到 “Binary content (...) saved to path”。见 `references/claude-code/src/services/mcp/client.ts:2594`, `:2604`, `:2605`, `:2619`, `references/claude-code/src/utils/mcpOutputStorage.ts:143`, `:148`, `:155`, `:181`。
- `ReadMcpResourceTool` 同样拦截 resource blob：写成本地文件，`blobSavedTo` 和 text path message 进入 result。见 `references/claude-code/src/tools/ReadMcpResourceTool/ReadMcpResourceTool.ts:103`, `:114`, `:115`, `:127`, `:131`。
- MCP persist 失败时不发送原文，而返回有界错误文字，建议使用 server pagination/filtering。见 `references/claude-code/src/services/mcp/client.ts:2775`, `:2783`。

### Cleanup / expiry / failure handling

- `tool-results` 没有 per-file TTL metadata 或 typed expired status；它们随 session cleanup 按 mtime 和 `cleanupPeriodDays` 删除，默认 30 天。见 `references/claude-code/src/utils/cleanup.ts:23`, `:27`, `:155`, `:196`, `:211`。
- 落盘失败行为不一致：通用 `maybePersistLargeToolResult()` 失败时返回原 block，可能把大内容送进模型；MCP 失败时返回短错误，不送原文；Bash copy/link 失败时只保留 stdout preview，注释认为 preview sufficient。见 `references/claude-code/src/utils/toolResultStorage.ts:316`, `references/claude-code/src/services/mcp/client.ts:2775`, `references/claude-code/src/tools/BashTool/BashTool.tsx:750`。
- Bash output 文件读取 ENOENT 时会返回诊断字符串，说明 output file 可能被另一个 Claude Code 进程启动清理删掉。见 `references/claude-code/src/utils/task/TaskOutput.ts:297`, `:312`。
- Background task output 文件超过 5GB 时 kill；pipe-mode `DiskTaskOutput` 超 cap 后写 truncation marker 并停止追加。见 `references/claude-code/src/utils/ShellCommand.ts:246`, `:318`, `references/claude-code/src/utils/task/diskOutput.ts:117`。
- resume 对缺失的 `tool-results` 文件没有独立状态建模；replacement 里保存的是 path + preview 文本。如果用户或模型 resume 后读取 path，是否存在由文件系统/Read 工具决定。

### Related specs / task context

- `.trellis/workflow.md` - Phase 1.2 要求 research 写入 `{TASK_DIR}/research/`，对话不是持久知识源。
- `.trellis/tasks/06-22-tool-large-output-lifecycle-draft/design.md` - 当前 AgentDash 设计目标是 producer 先裁切、SessionEvent 只保存 bounded fact、lifecycle ref 按需读取。
- `.trellis/tasks/06-22-tool-large-output-lifecycle-draft/implement.md` - 当前执行计划已包含 `LargeOutputPolicy`、cloud cache、terminal owner storage、stream/session projection、lifecycle VFS、frontend rendering 和验证项。

### External references

- 未联网检索。唯一外部来源是本仓库 `references/claude-code/README.md` 对快照来源的说明：该快照源于 2026-03-31 npm source map 暴露；技术栈为 TypeScript、Bun、React/Ink；该目录不是 Anthropic 官方仓库。

## Caveats / Not Found

- 未发现 Claude Code 对所有工具结果实施“持久化日志永不保存原文”的统一 invariant。模型可见 `tool_result.content` 有 offload/preview，但 JSONL transcript 仍可能保存 native `toolUseResult`；Bash/MCP 已在 native 层有界化，其他工具是否安全取决于各自 output schema。
- 未发现 typed `LargeOutputRef`/`LargeOutputTruncation` 一等 schema。Claude Code 主要用模型文本 wrapper、路径字符串、`persistedOutputPath`/`persistedOutputSize` 字段和 `content-replacement` record。
- 未发现 lifecycle VFS 风格的 stable ref、range metadata、digest、cache miss/expired typed status。Claude Code 的 ref 是本地文件路径，失败主要靠 Read/fs 错误或短错误文本表达。
- 未发现 terminal 输出的云端 relay/range read 设计。Claude Code 是本地 CLI，terminal/shell 输出 owner storage 基本就是本机 temp/session 文件。
- `references/claude-code` 是泄露快照研究材料，不能视为当前 Claude Code 产品行为或官方 API 契约。

## 可借鉴点

- 把“模型可见内容”与“完整原文 artifact”分离：大结果先落本地文件，tool_result 只放 preview、原始大小、读取路径。
- 对并发工具结果做 per-message aggregate budget；单个工具 under limit 不代表同一 user message 合计安全。
- replacement decision 按 `tool_use_id` 冻结，并把 replacement 文本写入 transcript，resume 时不重新生成 preview，保证 prompt cache 和上下文稳定。
- Bash/terminal 输出优先写文件 FD，JS/事件流只读取 head/tail/range；后台完成通知只给状态和 output path。
- 二进制/base64 结果必须先落 artifact，再用短文本指向路径；不要把 base64 当普通 text result。
- UI/search 不应索引模型专用 wrapper 文本，否则会出现用户看不见的搜索命中。

## 不可直接照搬点

- 直接把完整 artifact 路径暴露给模型不适合 AgentDash 云端/本机混合架构；需要 lifecycle ref + owner storage + relay/range read 权限边界。
- 通用落盘失败后返回原 block 的行为不适合作为安全边界；AgentDash 应该在 guard/persistence 层返回 typed bounded failure，而不是放行原文。
- transcript 仍保存 native `toolUseResult` 的做法不适合 AgentDash 的 `SessionEvent`/数据库目标；AgentDash 应把 bounded result 作为唯一 canonical carrier。
- 64MB Bash artifact truncate 和 5GB task disk cap 是 CLI 本机体验参数，不能直接映射到云端事件和浏览器 replay。
- `<persisted-output>` 文本 wrapper 可读但不利于前端、continuation、lifecycle resolver 结构化处理；AgentDash 应用 typed schema。

## 对 AgentDash implement.md 的具体建议

- 在 Phase 0 明确增加 “bounded canonical carrier replaces native output before any persistence” 验收项：`AgentToolResult.details.large_output` 之外，`notification_json`、raw stream event、frontend `rawEvents` 不得另存原始 native output。
- Phase 2/3 的 guard 需要覆盖 `toolUseResult` 等 UI/native 字段，而不只是模型 provider block；Claude Code 的边界说明了只替换模型 content 不够。
- Phase 3 增加 per-message aggregate budget 测试：多个 tool update/final result 在同一 turn 合并后仍 bounded。
- Phase 4 lifecycle ref metadata 应保存 `original_bytes`、`inline_bytes`、digest、storage kind、expires/status；不要只给路径字符串。
- Phase 5 terminal owner storage 可借鉴 “stdout/stderr 直接写 owner 文件，事件只读 tail/range”，但 completion notification 只能含 ref/status/summary，不能含 full tail 之外内容。
- Phase 7 新增 resume 验证：resume 后 replacement/ref 文本稳定，不重新生成不同 preview；ref 缺失/过期返回 typed bounded status；不会自动 follow ref 把原文重新放进模型上下文。

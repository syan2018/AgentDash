# inline_fs 图片资产支持

## Goal

让 `inline_fs` 可以作为 Project / Story / Agent Knowledge 等上下文容器中的轻量图片资产存储，用于保存截图、参考图、Logo、UI mock、任务说明配图等二进制图片文件，并在 VFS 浏览器中完成上传、预览、删除、重命名和引用。

目标不是把图片伪装成文本，而是把 `inline_fs` 从“仅文本文件”升级为“带内容类型的内联文件存储”，同时保持现有文本文件、Agent `fs.read` / `fs.write` / `fs.search` / `fs.apply_patch` 行为清晰可控。

## Confirmed Facts

- 当前 `inline_fs` 数据存储在 `inline_fs_files` 表，旧结构以 `content TEXT NOT NULL` 保存正文。
- `InlineFile` 领域实体、`InlineFileRepository`、`InlineContentOverlay`、`InlineFsMountProvider` 目前都以 `String` 文本内容为核心。
- VFS SPI 当前稳定文本接口为 `read_text` / `write_text`，`ReadResult.content` 是 `String`；`RuntimeFileEntry` 和 `ReadResult` 已有 `attributes` 元数据扩展位。
- Surface API 的 read/write/create 响应目前只返回 `content: string` 和以字符串长度计算的 `size`。
- 前端 VFS Browser 当前按文本编辑器体验读取文件内容，文件树只展示 path / size / is_dir。
- 项目处于预研期，允许做正确的数据结构迁移，不需要保留兼容性回退；但需要处理数据库 migration。
- 数据库规范要求新增迁移文件，不修改已提交 migration，并同步更新 `CREATE TABLE IF NOT EXISTS`、SELECT/INSERT/UPSERT、row mapping 与测试。

## Requirements

- `inline_fs` 必须支持文本文件和二进制图片文件共存。
- 图片资产必须保留 MIME 类型、字节大小、更新时间和路径。
- 文本文件仍然可以通过现有文本 VFS 工具读取、写入、搜索和 apply_patch。
- 二进制图片文件不得通过文本 `read_text` 返回 base64 大文本；文本读取二进制内容时应返回明确错误或通过 API 层返回明确的 binary metadata。
- `list` / `stat` 必须能区分文本与二进制条目，并通过 `attributes` 或 DTO 字段暴露 `content_kind` / `mime_type`。
- Surface API 必须提供图片上传和读取二进制内容的正式通道。
- 前端 VFS Browser 必须能在 `inline_fs` mount 中上传图片、选择图片、预览图片，并继续支持文本文件编辑。
- 文件创建、删除、重命名能力必须继续遵守现有 `edit_capabilities` 和权限检查。
- 不需要支持大规模对象存储能力；本任务面向轻量上下文图片资产。
- MVP 暂不要求 Agent 工具直接写入图片二进制内容；但存储模型、API 命名和 metadata 应避免阻塞后续“Agent 生成资产 → 会话 artifact → inline_fs 归档/搬运”的能力。

## Acceptance Criteria

- [ ] 新增 PostgreSQL migration，将 `inline_fs_files` 升级为可表达文本与二进制内容的 schema。
- [ ] 新建库初始化 SQL 与 repository SQL 保持与 migration 后 schema 一致。
- [ ] `InlineFile` 领域模型能表达 text / binary 两类内容，repository 能正确读写、列出、统计、删除。
- [ ] `InlineContentOverlay` 和 `InlineFsMountProvider` 对文本文件保持现有语义，对 binary 文件跳过文本 search，并在 text read/write/apply_patch 路径给出明确边界。
- [ ] Surface API 增加 binary upload/read/download 或等价 blob 通道，返回 `content_kind`、`mime_type`、`size` 等 metadata。
- [ ] Surface list/stat/read DTO 的字段使用 snake_case，前端 service 类型与后端 DTO 对齐。
- [ ] 前端 VFS Browser 能上传 PNG/JPEG/WebP/GIF/SVG 等图片文件到 `inline_fs`，文件树刷新后可选择并预览。
- [ ] 前端文本文件仍使用 CodeMirror 编辑，图片文件不进入文本编辑器。
- [ ] 现有 inline text 文件相关测试继续通过，并新增 binary inline file repository / provider / API / 前端行为测试。
- [ ] 运行并记录至少以下验证：相关 Rust tests、前端 type-check / test，以及必要的 VFS browser 手动或自动验证。

## Out Of Scope

- 不建设跨 Project 的通用媒体库。
- 不接入 S3 / MinIO / 外部对象存储。
- 不为非图片二进制文件做专门预览器；可保留通用下载或 metadata 展示。
- 不要求 Agent 文本工具直接消费图片二进制内容。
- 不处理历史线上数据兼容回退；项目未上线，只保留 migration 和新建库正确性。

## Open Questions

- 已决策：MVP 先支持前端/API 上传、预览和下载；Agent 侧先以 VFS URI 引用图片，不在本任务内扩展 Agent tool 的二进制写入。
- 已创建长期讨论任务：`05-20-session-artifact-asset-ops`，用于规划 Agent 生成图片等 session artifact 的归档、复制、搬运与受控资产维护能力。

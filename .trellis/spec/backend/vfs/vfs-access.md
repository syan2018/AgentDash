# 统一 VFS（跨层契约）

> Agent 和上层用例不直接感知 `backend_id + absolute path`，统一使用 `mount + relative path` 模型。

---

## 核心设计

所有资源访问统一为 `mount + relative path`：

- `mount` 是会话级挂载 ID（如 `main` / `spec` / `brief` / `lifecycle`）
- `path` 是相对 mount 根的路径
- 每个 session 启动时生成一份 mount table

Application 层的最小地址类型包括：

- `MountId`
- `MountRelativePath`
- `VfsUri`
- `RootRef::LocalPath | RootRef::ProviderUri`
- `PathPolicy`

原始字符串只能存在于 UI/API/relay/tool 输入边界；进入 application 内部前必须 parse/normalize 成结构化地址。

## 运行时工具集

稳定的小工具集合：

- `mounts.list` — 列出当前会话可访问的 mount 清单
- `fs.read` / `fs.write` / `fs.apply_patch` — 文件读写
- `fs.list` / `fs.search` — 目录和内容搜索
- `shell.exec` — 命令执行（仅限声明了 `exec` 能力的 mount）

所有工具使用统一参数模型：`{ "mount": "main", "path": "relative/path" }`

## Provider 能力矩阵

| 能力 | 物理 workspace | KM / Snapshot | Lifecycle VFS |
| --- | --- | --- | --- |
| `read` | 必须 | 必须 | 必须 |
| `write` | 可选 | 按 provider | 受限（artifacts/records） |
| `list` | 必须 | 必须 | 必须 |
| `search` | 推荐 | 推荐 | — |
| `exec` | 可选 | 不支持 | 不支持 |
| `watch` | — | — | 可选（通知机制） |

当前已落地的 provider：
- `relay_fs`：通过 relay 访问本机物理工作空间
- `inline_fs`：云端 Project/Story 配置导出的内联只读文件
- `lifecycle_vfs`：将 LifecycleRun 暴露为虚拟文件系统

## 错误矩阵

| 条件 | 错误语义 |
| --- | --- |
| mount 不存在 | `NotFound` |
| path 为绝对路径或含 `..` 越界 | `InvalidPath` / `PathEscapesMount` |
| mount 不支持该能力 | `CapabilityDenied` |
| 目标 backend 不在线 | `BackendOffline` |
| relay 超时 | `Timeout` |

## 关键契约

1. **资源定位**：Agent 不应直接感知 `backend_id` 或绝对路径
2. **一致性**：声明式来源解析与运行时工具访问共享同一套 provider 底座
3. **relay 隔离**：relay 是 transport，不是 mount 模型；上层不直接拼接 `RelayMessage`
4. **写入约束**：`fs.write` 默认只允许 `default_write=true` 的 mount；`fs.apply_patch` 受 `write` 能力约束
5. **物化路径**：VFS URI 转本机 path 必须遵守 [vfs-materialization.md](./vfs-materialization.md)
6. **root_ref 类型化**：`RootRef` 必须区分本机路径和 provider URI；`lifecycle://`、`skill-assets://`、`canvas://` 等虚拟 root 不得隐式转为 OS `PathBuf`
7. **路径硬校验**：`Vfs` 构建/派生后必须执行 hard validation，至少检查 mount id 唯一、系统保留 mount id 未被错误 provider 占用、default mount 存在、root_ref/provider scheme 合法、内置 provider capability 与支持范围一致、link target 存在且无环
8. **预研期约束**：不保留旧路径行为回退；发现非法地址应直接失败并补测试

## 内核扩展能力

### Extended Attributes

Provider 可在 `read` / `list` / `stat` 结果中附带结构化元数据（`attributes` 字段），替代在文件内容开头嵌入 YAML frontmatter 的做法。

## Scenario: inline_fs 图片资产与二进制内容

### 1. Scope / Trigger

- Trigger: `inline_fs` 从 text-only 文件扩展为 text / binary typed inline file storage。
- Scope: `inline_fs_files` DB schema、`InlineFile` domain model、Surface VFS API、前端 VFS Browser。
- Non-goal: Agent `fs.read` / `fs.write` 不直接读写图片二进制；Agent 生成图片归档另走 session artifact promotion 设计。

### 2. Signatures

DB `inline_fs_files` 内容列：

```sql
content_kind   TEXT NOT NULL, -- 'text' | 'binary'
mime_type      TEXT,
text_content   TEXT,
binary_content BYTEA,
size_bytes     BIGINT NOT NULL
```

Domain content model:

```rust
pub enum InlineFileContent {
    Text { content: String },
    Binary { bytes: Vec<u8>, mime_type: String },
}
```

Surface API additions:

```text
POST /api/vfs-surfaces/read-file-blob
POST /api/vfs-surfaces/upload-file-blob
```

### 3. Contracts

- `content_kind = "text"`: `text_content IS NOT NULL` and `binary_content IS NULL`.
- `content_kind = "binary"`: `binary_content IS NOT NULL`, `text_content IS NULL`, and `mime_type IS NOT NULL`.
- `list` / `stat` for inline files must expose:
  - `size`
  - `attributes.content_kind`
  - `attributes.mime_type`
- Surface list/stat DTO mirrors those fields as `content_kind` / `mime_type` in snake_case.
- Text APIs (`read-file`, `write-file`, `create-file`, `apply-patch`) remain text APIs.
- Blob APIs are limited to `inline_fs` mounts unless a future provider explicitly implements binary support.

### 4. Validation & Error Matrix

| Condition | Error |
| --- | --- |
| `read_text` on binary inline file | `MountError::NotSupported` |
| `search_text` sees binary inline file | Skip file |
| `apply_patch` targets binary inline file | Text read fails; do not coerce binary to text |
| Blob read on text file | HTTP 400 |
| Blob read/upload on non-`inline_fs` mount | HTTP 400 |
| Upload MIME is not `image/*` | HTTP 400 |
| Invalid mount-relative path | HTTP 400 via path normalization |

### 5. Good/Base/Bad Cases

- Good: Upload `assets/logo.png` as `image/png`; list returns `content_kind=binary`, `mime_type=image/png`, `size=<bytes>`; VFS Browser renders image preview.
- Base: Existing `note.md` remains `content_kind=text`; `fs.read` and CodeMirror editing still work.
- Bad: Store image as base64 in `text_content`; this pollutes search/apply_patch/editor behavior and must not be used as the storage model.

### 6. Tests Required

- Provider unit test: binary inline file appears in list/stat metadata.
- Provider unit test: `read_text` rejects binary and `search_text` skips binary.
- API test: multipart image upload writes binary row and blob read returns bytes with `Content-Type`.
- Frontend test: VFS Browser routes image entries to blob preview, not `VfsCodeEditor`.

### 7. Wrong vs Correct

#### Wrong

```rust
let file = InlineFile::new(owner_kind, owner_id, "brief", "logo.png", base64_image);
```

#### Correct

```rust
let file = InlineFile::new_binary(
    owner_kind,
    owner_id,
    "brief",
    "assets/logo.png",
    png_bytes,
    "image/png",
);
```

### Projection（虚拟文件）

`is_virtual=true` 标记条目是 provider 动态投影的内容（物理存储中不存在），如 `lifecycle_vfs` 的 `active/*`、`nodes/*`、`runs/*` 路径。

### Mount Link

声明式的 mount 级重定向（不是通用 symlink），`parse_mount_uri` 自动跳转。最大跳转深度 5 层。用于 workflow step input 引用上游 output、共享文档引用等场景。

### Watch / 事件通知

Provider 可通过 broadcast channel 推送 `MountEvent`（Created/Modified/Deleted/Renamed），供 Application 层内部消费（Workflow 编排、Hook runtime），暂不在 Agent tool 层暴露。

---

*创建：2026-04-17 — 统一 VFS 跨层契约*
*精简：2026-05-16 — 移除代码复述、测试列表、实施计划；保留核心契约和能力矩阵*
*更新：2026-05-16 — 补充资源地址类型、root_ref 类型化与 VFS hard validation 契约*
*更新：2026-05-20 — 补充 inline_fs text/binary 内容契约与图片资产 API 边界*

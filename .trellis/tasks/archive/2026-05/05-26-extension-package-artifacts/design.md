# Design

## Storage Model

推荐新增 artifact 记录，字段至少包含：

- id
- project_id 或 owner scope
- extension_id
- package_name
- package_version
- asset_version
- storage_ref
- archive_digest
- manifest_digest
- manifest snapshot
- created_at / updated_at

首版 storage 可以复用后端现有文件/asset 存储策略；若没有统一 blob store，则以数据库 metadata + local server storage ref 起步。

## Install Flow

```text
agentdash-ext install
  -> upload archive
  -> validate manifest and digest
  -> create artifact record
  -> create/update ProjectExtensionInstallation
  -> installed source records artifact digest/storage ref
```

Project runtime 读取 Project extension installation，不直接读取 Shared Library raw payload。

## Local Cache

`agentdash-local` 按 Project/session 需要下载 artifact：

- 下载前检查 cache key：artifact id + digest。
- 下载后校验 digest。
- 解包到 cache。
- Extension Host 从 cache path 加载 `dist/extension.js`。

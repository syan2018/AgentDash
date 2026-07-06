# 工作项 1：发布产物与 manifest 契约

## Goal

主仓生成对象存储无关的桌面发布目录，包含人工安装包、Tauri updater artifact、signature、sha256、release manifest、stable latest manifest 和 upload plan。

## Scope

- 接入或配置 Tauri updater artifact 产出。
- 扩展 release metadata，按平台描述桌面 artifacts。
- 生成稳定目录结构，默认覆盖 Windows x86_64 stable channel。
- 生成 upload plan，供私有仓或 CI/CD 同步到 S3-compatible 对象存储。
- 文档只写通用对象存储契约和环境变量占位，不写企业真实 endpoint、bucket、AK/SK 或私有域名。

## Deliverables

- `dist/release/desktop` 或等价发布目录结构。
- `release.json`：版本级 manifest。
- `channels/stable/latest.json`：stable latest 指针 manifest。
- `upload-plan.json`：本地文件到 object key / public URL 的映射。
- sha256 文件或 manifest 内嵌 sha256。
- updater artifact signature。
- 发布 Runbook 中的通用发布目录说明。

## Checkpoints

- [x] 发布命令能定位人工安装包和 updater artifact。
- [x] 每个 artifact 都有 sha256。
- [x] updater artifact 有 signature。
- [x] manifest schema 按 platform / arch 建模，没有把 Windows 写死到顶层事实模型。
- [x] upload plan 不包含企业真实 endpoint、bucket、AK/SK 或私有域名。
- [x] `channels/stable/latest.json` 只引用已存在的 versioned artifacts。
- [x] release metadata 脚本有测试覆盖基础字段、平台矩阵和缺失 artifact 错误。

## Suggested Validation

- `pnpm run release:metadata`
- release metadata 脚本相关测试
- `pnpm run desktop:bundle` 或可替代的 artifact fixture 验证

## Check Result

- 已通过 `node --test scripts/lib/release-metadata.test.js` 和 `pnpm run release:metadata`。
- 当前使用 fixture 覆盖 updater artifact / signature 发现逻辑；真实 Tauri bundle 产物验证留到发布环境执行。

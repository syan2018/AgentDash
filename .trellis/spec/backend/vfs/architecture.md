# VFS Architecture

## Role

VFS 子系统给 Agent、前端和业务用例提供统一地址模型，屏蔽 `backend_id`、绝对路径、数据库主键和 inline storage 坐标。它负责 surface resolution、provider dispatch、runtime mount、mutation 和本机物化边界。

## Invariants

- 外部访问地址统一为 `surface_ref + mount_id + mount_relative_path`。
- `mount_relative_path` 进入 application 层前必须 normalize；绝对路径和 `..` escape 必须失败。
- 云端 provider 不直接访问本机文件系统；本机 provider 不直接读写业务数据库。
- runtime mount 是 provider 分发单位，至少包含 id、provider、root_ref、capabilities 和 metadata。
- `Vfs` 构建后必须 hard validate：mount id 唯一、default mount 存在、provider/root_ref 合法、capability 与 provider 支持范围一致、link 无环。
- Inline storage 坐标只能由 application resolver 从 runtime mount metadata 生成。
- binary bytes 不内联进 JSON DTO；通过 `read_binary` / blob 通道读取。

## Current Baseline

Provider baseline：

| Provider | 职责 |
| --- | --- |
| `relay_fs` | 通过 relay 访问本机 workspace 文件 |
| `inline_fs` | 暴露 Project / Story / Agent Knowledge 等内联文件 |
| `skill_asset_fs` | 暴露 Skill asset 文件视图 |
| `lifecycle_vfs` | 暴露 lifecycle run、node、artifact、record 投影 |
| `canvas_fs` | 暴露 Canvas 虚拟内容 |

## Local Decisions

- Project VFS Mount 使用外部 `mount_id` 作为路径身份，数据库 UUID 只服务持久化和 inline owner，原因是 runtime address 必须稳定可读。
- VFS 物化默认使用公共稳定路径，只有语义明确绑定 session 的动态投影进入 session scope，原因是公共资源需要跨 session 复用。

## Contract Appendices

- [VFS Access](./vfs-access.md)
- [VFS Materialization](./vfs-materialization.md)


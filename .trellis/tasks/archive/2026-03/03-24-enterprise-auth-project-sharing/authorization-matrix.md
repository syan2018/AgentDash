# 授权矩阵

## 角色定义

### `admin_bypass`

- 系统管理员旁路能力
- 可查看 / 管理所有 Project
- 不依赖 Project grants

### `owner`

- 管理 Project 基础信息
- 管理成员与共享
- 管理模板属性
- 创建 / 修改 / 删除下游业务对象

### `editor`

- 创建 / 修改 Story、Task、Session、WorkflowRun
- 可读 Workspace 文件
- 不可管理分享策略
- 不可删除或重置核心 Project 权限配置

### `viewer`

- 只读查看 Project 内容
- 可查看 Story、Task、Session、WorkflowRun、实时状态
- 不可修改业务对象
- 不可触发会改变共享数据的写操作

## 资源矩阵

| 资源 | viewer | editor | owner | admin_bypass |
|---|---|---|---|---|
| Project 基本信息 | 读 | 读/部分改 | 全部 | 全部 |
| Project 分享管理 | 否 | 否 | 是 | 是 |
| Story | 读 | 读写 | 读写 | 全部 |
| Task | 读 | 读写 | 读写 | 全部 |
| Session | 读 | 读写/交互 | 读写/管理 | 全部 |
| WorkflowRun | 读 | 读写 | 读写 | 全部 |
| Workspace 文件读取 | 读 | 读 | 读 | 全部 |
| Workspace 文件写入 | 否/受执行链控制 | 受执行链控制 | 受执行链控制 | 全部 |
| Settings.system | 否 | 否 | 否 | 是 |
| Settings.user | 自己 | 自己 | 自己 | 全部 |
| Settings.project | 读 | 读写 | 读写 | 全部 |
| 模板 clone | 是 | 是 | 是 | 是 |

## Session 规则

- Project Agent Session：任何拥有 Project 读权限的用户可见
- Story Companion Session：任何拥有 Story 所属 Project 读权限的用户可见
- Task Execution Session：任何拥有 Task 所属 Project 读权限的用户可见
- Session 的“可见”与“可交互”可复用 Project 角色：
  - viewer：只读看历史与状态
  - editor/owner：可 prompt / continue / cancel / approve 等

## 模板规则

- 模板通常允许被组织内用户看到并 clone
- clone 后得到新的普通 Project
- 新 Project 默认只授予创建者 `owner`
- clone 不应复制原 Project 的 grants

## 个人模式说明

- 个人模式可以退化为：
  - 单用户直通
  - 或少量本地用户配置
- 但在权限模型上仍使用同一套角色与 Project grants 语义
- 不单独引入“个人模式专属 Session 规则”

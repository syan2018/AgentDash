# Relay 与 Address Space 边界加固

## 目标

- 收紧 companion compact slice 的权限语义。
- 让 workspace 检测、shell exec 等路径重新回到统一边界内。
- 消除 relay capability 声明与实现之间的漂移。

## 非目标

- 不在本任务内设计全新协议版本。
- 不对所有 provider 系统做全面重命名迁移。

## 验收标准

- `compact` 模式权限与 adoption 语义一致，不可轻易绕穿。
- `workspace_detect_git` 不再绕过 `accessible_roots`。
- `discover_options` 能力要么真实实现，要么从注册能力中移除。

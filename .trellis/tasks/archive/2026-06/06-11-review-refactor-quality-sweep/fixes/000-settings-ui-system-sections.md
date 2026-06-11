# FIX-000: settings-ui system sections 预备重构

## 模块

`settings-ui`

## 背景

`packages/app-web/src/features/settings/ui/SettingsSystemSections.tsx` 曾触发 `fuck-u-code analyze packages/app-web/src/features/settings/ui` timeout。文件内 `BackendSection` 将 view model 构造、列表项摘要、展开详情、状态样式和删除动作塞入单个 JSX map 回调，形成长嵌套和裸字段长程传递。

## 更新

- 拆分 `BackendSection` 的 view model、列表项、详情区和辅助渲染组件。
- 使用 `@agentdash/ui` 的 `Button`、`Select`、`Textarea`、`Badge`、`StatusDot` 收敛业务侧样式。
- 保留现有导出组件接口。

## 验证

- `pnpm --filter app-web run typecheck`：通过。
- `pnpm --filter app-web run lint`：通过，仅剩既有 `SessionChatViewParts.tsx` warning。
- `fuck-u-code analyze packages/app-web/src/features/settings/ui -l zh -f markdown -o ... -t 10 -c 1 -e **/*.test.ts -e **/*.test.tsx`：通过，167ms 完成。

## Commit

`9c7999a0 refactor(settings): 收敛系统设置区块组件结构`

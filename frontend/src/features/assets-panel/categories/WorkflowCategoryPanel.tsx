/**
 * WorkflowCategoryPanel — Assets 页 Workflow 类目占位。
 *
 * 本 PR（PR3）仅渲染占位。列表 / 预览 / 跳转编辑器 留到 PR4 实装。
 */
export function WorkflowCategoryPanel() {
  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <header className="space-y-1">
        <h2 className="text-base font-semibold tracking-tight text-foreground">Workflow 资产</h2>
        <p className="text-xs text-muted-foreground">
          项目级可复用的 Workflow / Lifecycle 模板。支持 builtin 与 user 两种来源，Lifecycle DAG 可缩略预览。
        </p>
      </header>

      <div className="flex flex-1 items-center justify-center rounded-[12px] border border-dashed border-border bg-secondary/30 p-6">
        <div className="max-w-md text-center">
          <p className="text-sm font-medium text-foreground">Workflow 资产列表将在 PR4 实装</p>
          <p className="mt-2 text-xs text-muted-foreground">
            届时接入 `workflowStore` 的 `definitions` / `lifecycleDefinitions` 与 builtin template 列表，支持"编辑"跳转到 `workflow-editor` / `lifecycle-editor` 子路由。
          </p>
        </div>
      </div>
    </div>
  );
}

export default WorkflowCategoryPanel;

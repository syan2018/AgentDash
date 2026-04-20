/**
 * McpPresetCategoryPanel — Assets 页 MCP Preset 类目占位。
 *
 * 本 PR（PR3）仅渲染占位。MCP Preset 列表 + 就地表单 CRUD + builtin 只读态 留到 PR5 实装。
 */
export function McpPresetCategoryPanel() {
  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <header className="space-y-1">
        <h2 className="text-base font-semibold tracking-tight text-foreground">MCP Preset 资产</h2>
        <p className="text-xs text-muted-foreground">
          项目级可复用的 MCP Server 模板（http / sse / stdio 三种 transport）。支持 builtin 与 user 两种来源。
        </p>
      </header>

      <div className="flex flex-1 items-center justify-center rounded-[12px] border border-dashed border-border bg-secondary/30 p-6">
        <div className="max-w-md text-center">
          <p className="text-sm font-medium text-foreground">MCP Preset CRUD 将在 PR5 实装</p>
          <p className="mt-2 text-xs text-muted-foreground">
            届时接入 `frontend/src/services/mcpPreset.ts` 的 client，支持 builtin 只读预览 / 复制为 user / user CRUD 全流程。
          </p>
        </div>
      </div>
    </div>
  );
}

export default McpPresetCategoryPanel;

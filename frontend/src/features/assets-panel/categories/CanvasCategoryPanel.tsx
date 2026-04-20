/**
 * CanvasCategoryPanel — Assets 页 Canvas 类目占位。
 *
 * 本 PR（PR3）仅渲染占位。Canvas 资产列表 / 缩略预览 / 跳转 `CanvasTabView` 编辑留到 PR4 实装。
 */
export function CanvasCategoryPanel() {
  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <header className="space-y-1">
        <h2 className="text-base font-semibold tracking-tight text-foreground">Canvas 资产</h2>
        <p className="text-xs text-muted-foreground">
          项目级可视化 Canvas 模板，展示 files 计数 / bindings 计数 / 最近编辑时间。编辑时跳回原 Canvas 面板。
        </p>
      </header>

      <div className="flex flex-1 items-center justify-center rounded-[12px] border border-dashed border-border bg-secondary/30 p-6">
        <div className="max-w-md text-center">
          <p className="text-sm font-medium text-foreground">Canvas 资产列表将在 PR4 实装</p>
          <p className="mt-2 text-xs text-muted-foreground">
            届时接入 `fetchProjectCanvases` 的 `Canvas[]`，支持"编辑"跳转到 `CanvasTabView` 下的项目级 Canvas 编辑面板。
          </p>
        </div>
      </div>
    </div>
  );
}

export default CanvasCategoryPanel;

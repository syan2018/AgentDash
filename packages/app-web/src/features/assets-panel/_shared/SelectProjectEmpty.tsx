/**
 * 空 projectId 时的占位文案。所有 CategoryPanel 都用同一个外观与措辞。
 */
export function SelectProjectEmpty({ assetLabel }: { assetLabel: string }) {
  return (
    <div className="flex h-full items-center justify-center p-6">
      <div className="text-center text-sm text-muted-foreground">
        请选择项目后查看 {assetLabel}
      </div>
    </div>
  );
}

export default SelectProjectEmpty;

export function PublishedBadge({ version }: { version: string }) {
  return (
    <span
      className="shrink-0 rounded-[6px] border border-violet-500/30 bg-violet-500/10 px-1.5 py-0.5 text-[10px] font-medium text-violet-700 dark:text-violet-300"
      title="此资产已发布到资源市场"
    >
      已发布 v{version}
    </span>
  );
}

import { Badge } from '@agentdash/ui'

export function PublishedBadge({ version }: { version: string }) {
  return (
    <Badge
      variant="accent"
      className="shrink-0 rounded-[6px] px-1.5 py-0.5 text-[10px] min-h-0"
      title="此资产已发布到资源市场"
    >
      已发布 v{version}
    </Badge>
  );
}

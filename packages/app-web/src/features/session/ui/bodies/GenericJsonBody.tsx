/**
 * 通用 JSON 入参/出参双分区展示
 *
 * 兜底 body：对任何未注册专用 renderer 的工具调用，
 * 以可折叠 JSON 树展示 arguments 和 contentItems。
 */

import { JsonTree, CopyJsonButton } from "./JsonTree";

export interface GenericJsonBodyProps {
  arguments?: unknown;
  contentItems?: unknown;
}

export function GenericJsonBody({ arguments: args, contentItems }: GenericJsonBodyProps) {
  const hasArgs = args != null && !(typeof args === "object" && args !== null && Object.keys(args).length === 0);
  const hasOutput = contentItems != null;

  if (!hasArgs && !hasOutput) return null;

  return (
    <div className="space-y-3">
      {hasArgs && (
        <Section label="入参" data={args}>
          <JsonTree data={args} defaultDepth={2} />
        </Section>
      )}
      {hasOutput && (
        <Section label="出参" data={contentItems}>
          <JsonTree data={contentItems} defaultDepth={1} />
        </Section>
      )}
    </div>
  );
}

function Section({
  label,
  data,
  children,
}: {
  label: string;
  data: unknown;
  children: React.ReactNode;
}) {
  return (
    <div>
      <div className="mb-1 flex items-center justify-between">
        <p className="text-xs font-medium text-muted-foreground/60">{label}</p>
        <CopyJsonButton data={data} />
      </div>
      {children}
    </div>
  );
}

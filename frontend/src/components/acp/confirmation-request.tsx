import type { ConfirmationRequest } from "../../types";

export function ConfirmationRequestCard({ request }: { request: ConfirmationRequest }) {
  return (
    <div className="rounded-md border border-warning/30 bg-warning/10 p-3">
      <p className="text-xs text-muted-foreground">{request.requestKind}</p>
      <p className="mt-1 text-sm font-medium text-foreground">{request.title}</p>
      {request.description && <p className="mt-1 text-sm text-muted-foreground">{request.description}</p>}
      <p className="mt-2 text-xs text-muted-foreground">发起时间：{new Date(request.createdAt).toLocaleString("zh-CN")}</p>
    </div>
  );
}

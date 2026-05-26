/**
 * Web 搜索 body — 搜索 query + action 摘要
 */

import type { ThreadItem } from "../../../../generated/backbone-protocol";

type WebSearchItem = Extract<ThreadItem, { type: "webSearch" }>;

export function WebSearchCardBody({ item }: { item: WebSearchItem }) {
  return (
    <div className="space-y-2 text-xs">
      <div>
        <p className="mb-1 text-muted-foreground/60 font-medium">查询</p>
        <p className="text-foreground">{item.query}</p>
      </div>
      {item.action && (
        <div>
          <p className="mb-1 text-muted-foreground/60 font-medium">操作</p>
          <p className="font-mono text-foreground/80">{describeAction(item.action)}</p>
        </div>
      )}
    </div>
  );
}

function describeAction(action: NonNullable<WebSearchItem["action"]>): string {
  switch (action.type) {
    case "search":
      return action.queries?.join(", ") ?? action.query ?? "搜索";
    case "openPage":
      return action.url ?? "打开页面";
    case "findInPage":
      return `在 ${action.url ?? "页面"} 中查找 ${action.pattern ?? ""}`;
    case "other":
      return "其他操作";
    default:
      return "未知操作";
  }
}

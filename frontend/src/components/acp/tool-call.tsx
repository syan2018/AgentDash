import { useState } from "react";
import type { ToolCall } from "../../types";

function safeJson(value: unknown) {
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

export function ToolCallView({ toolCall }: { toolCall: ToolCall }) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="rounded-md border border-border bg-card">
      <button
        type="button"
        onClick={() => setExpanded((prev) => !prev)}
        className="flex w-full items-center justify-between px-3 py-2 text-left text-sm"
      >
        <span className="font-medium text-foreground">{toolCall.title}</span>
        <span className="text-xs text-muted-foreground">{toolCall.status ?? "pending"}</span>
      </button>
      {expanded && (
        <div className="space-y-2 border-t border-border px-3 py-2">
          {toolCall.rawInput !== undefined && (
            <div>
              <p className="mb-1 text-xs text-muted-foreground">Input</p>
              <pre className="overflow-auto rounded-md bg-secondary/40 p-2 text-xs">{safeJson(toolCall.rawInput)}</pre>
            </div>
          )}
          {toolCall.rawOutput !== undefined && (
            <div>
              <p className="mb-1 text-xs text-muted-foreground">Output</p>
              <pre className="overflow-auto rounded-md bg-secondary/40 p-2 text-xs">{safeJson(toolCall.rawOutput)}</pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

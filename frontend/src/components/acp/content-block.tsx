import type { ContentBlock } from "../../types";

function renderJson(value: unknown) {
  return JSON.stringify(value, null, 2);
}

export function ContentBlockView({ block }: { block: ContentBlock }) {
  if (block.type === "text") {
    return <p className="whitespace-pre-wrap text-sm leading-relaxed text-foreground">{block.text}</p>;
  }
  if (block.type === "image") {
    return (
      <img
        alt=""
        className="max-h-72 max-w-full rounded-md border border-border"
        src={`data:${block.mimeType};base64,${block.data}`}
      />
    );
  }
  if (block.type === "resource_link") {
    return (
      <div className="rounded-md border border-border bg-card px-3 py-2 text-xs">
        <p className="font-mono text-foreground">{block.name}</p>
        <p className="mt-1 text-muted-foreground">{block.uri}</p>
      </div>
    );
  }
  return (
    <pre className="overflow-auto rounded-md border border-border bg-card p-3 text-xs text-foreground">
      {renderJson(block.resource)}
    </pre>
  );
}

export function ContentBlockList({ blocks }: { blocks: ContentBlock[] }) {
  if (blocks.length === 0) return null;
  return (
    <div className="space-y-2">
      {blocks.map((block, index) => (
        <ContentBlockView key={index} block={block} />
      ))}
    </div>
  );
}

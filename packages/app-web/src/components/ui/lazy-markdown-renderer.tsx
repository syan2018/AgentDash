import { lazy, Suspense } from "react";
import type { MarkdownRendererProps } from "./markdown-renderer";

const MarkdownRendererImpl = lazy(() => import("./markdown-renderer"));

export function LazyMarkdownRenderer(props: MarkdownRendererProps) {
  return (
    <Suspense
      fallback={
        <div className="agentdash-markdown whitespace-pre-wrap text-sm leading-7 text-foreground">
          {props.content}
        </div>
      }
    >
      <MarkdownRendererImpl {...props} />
    </Suspense>
  );
}

export default LazyMarkdownRenderer;

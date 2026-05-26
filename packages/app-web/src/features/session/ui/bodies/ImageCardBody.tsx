/**
 * 图片查看/生成 body
 */

import type { ThreadItem } from "../../../../generated/backbone-protocol";

type ImageViewItem = Extract<ThreadItem, { type: "imageView" }>;
type ImageGenItem = Extract<ThreadItem, { type: "imageGeneration" }>;

export function ImageCardBody({ item }: { item: ImageViewItem | ImageGenItem }) {
  if (item.type === "imageView") {
    return (
      <div className="text-xs">
        <p className="mb-1 text-muted-foreground/60 font-medium">路径</p>
        <p className="font-mono text-foreground/80 break-all">{item.path}</p>
      </div>
    );
  }

  return (
    <div className="space-y-2 text-xs">
      {item.revisedPrompt && (
        <div>
          <p className="mb-1 text-muted-foreground/60 font-medium">提示词</p>
          <p className="text-foreground/80">{item.revisedPrompt}</p>
        </div>
      )}
      {item.result && (
        <div>
          <p className="mb-1 text-muted-foreground/60 font-medium">结果</p>
          <p className="font-mono text-foreground/80 break-all">{item.result}</p>
        </div>
      )}
      {item.savedPath && (
        <div>
          <p className="mb-1 text-muted-foreground/60 font-medium">保存路径</p>
          <p className="font-mono text-foreground/80 break-all">{item.savedPath}</p>
        </div>
      )}
    </div>
  );
}

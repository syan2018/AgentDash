/**
 * 图片查看/生成 body
 */

import type { ThreadItem } from "../../../../generated/backbone-protocol";
import { CB } from "./cardBodyTokens";

type ImageViewItem = Extract<ThreadItem, { type: "imageView" }>;
type ImageGenItem = Extract<ThreadItem, { type: "imageGeneration" }>;

export function ImageCardBody({ item }: { item: ImageViewItem | ImageGenItem }) {
  if (item.type === "imageView") {
    return (
      <div className="text-xs">
        <p className={`mb-0.5 ${CB.sectionTitle}`}>路径</p>
        <p className="font-mono text-foreground/80 break-all">{item.path}</p>
      </div>
    );
  }

  return (
    <div className={`${CB.sectionGap} text-xs`}>
      {item.revisedPrompt && (
        <div>
          <p className={`mb-0.5 ${CB.sectionTitle}`}>提示词</p>
          <p className="text-foreground/80">{item.revisedPrompt}</p>
        </div>
      )}
      {item.result && (
        <div>
          <p className={`mb-0.5 ${CB.sectionTitle}`}>结果</p>
          <p className="font-mono text-foreground/80 break-all">{item.result}</p>
        </div>
      )}
      {item.savedPath && (
        <div>
          <p className={`mb-0.5 ${CB.sectionTitle}`}>保存路径</p>
          <p className="font-mono text-foreground/80 break-all">{item.savedPath}</p>
        </div>
      )}
    </div>
  );
}

import { useState } from "react";
import { CB } from "./bodies/cardBodyTokens";
import { projectDiagnosticText } from "./boundedDiagnosticTextModel";

export interface BoundedDiagnosticDisclosureProps {
  text: string;
  label: string;
  tone?: "neutral" | "danger";
  showPreview?: boolean;
}

export function BoundedDiagnosticDisclosure({
  text,
  label,
  tone = "neutral",
  showPreview = true,
}: BoundedDiagnosticDisclosureProps) {
  const normalizedText = text.trim();
  const [expanded, setExpanded] = useState(false);
  if (normalizedText.length === 0) return null;

  const projection = projectDiagnosticText(normalizedText);
  const buttonClass = tone === "danger"
    ? "text-destructive/70 hover:bg-destructive/10"
    : "text-muted-foreground/60 hover:bg-secondary/30";
  const previewClass = tone === "danger"
    ? "text-destructive/70"
    : "text-muted-foreground/70";

  return (
    <div className="mt-2 space-y-1">
      <button
        type="button"
        onClick={() => setExpanded((value) => !value)}
        className={`rounded-[4px] px-1.5 py-0.5 text-left text-[10px] transition-colors ${buttonClass}`}
      >
        {expanded ? "收起" : "展开"}{label}
        <span className="ml-2 font-mono opacity-60">{normalizedText.length} chars</span>
      </button>
      {!expanded && showPreview && (
        <p className={`whitespace-pre-wrap break-words text-xs leading-5 ${previewClass}`}>
          {projection.summary}
        </p>
      )}
      {expanded && (
        <pre className={`max-h-64 overflow-auto whitespace-pre-wrap break-words ${CB.codeBlock}`}>
          {normalizedText}
        </pre>
      )}
    </div>
  );
}

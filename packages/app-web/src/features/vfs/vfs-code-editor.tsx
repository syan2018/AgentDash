/**
 * VFS 文件内容编辑器
 *
 * 基于 CodeMirror 6 的文件查看/编辑组件，
 * 根据文件扩展名自动选择语言高亮。
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { syntaxHighlighting, defaultHighlightStyle, bracketMatching } from "@codemirror/language";
import { searchKeymap, highlightSelectionMatches } from "@codemirror/search";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { css } from "@codemirror/lang-css";
import { html } from "@codemirror/lang-html";
import { markdown } from "@codemirror/lang-markdown";
import { rust } from "@codemirror/lang-rust";
import { python } from "@codemirror/lang-python";
import type { Extension } from "@codemirror/state";
import { LazyMarkdownRenderer } from "../../components/ui/lazy-markdown-renderer";

type MarkdownViewMode = "edit" | "preview" | "split";

const lightTheme = EditorView.theme({
  "&": {
    backgroundColor: "var(--color-background, #ffffff)",
    color: "var(--color-foreground, #1e293b)",
    fontSize: "12px",
    fontFamily: "ui-monospace, 'Cascadia Code', 'Fira Code', 'JetBrains Mono', Menlo, Consolas, monospace",
    height: "100%",
  },
  ".cm-scroller": {
    overflow: "auto",
  },
  ".cm-gutters": {
    backgroundColor: "var(--color-secondary, #f8fafc)",
    color: "var(--color-muted-foreground, #94a3b8)",
    borderRight: "1px solid var(--color-border, #e2e8f0)",
  },
  ".cm-activeLineGutter": {
    backgroundColor: "color-mix(in srgb, var(--color-primary, #3b82f6) 8%, transparent)",
  },
  ".cm-activeLine": {
    backgroundColor: "color-mix(in srgb, var(--color-primary, #3b82f6) 5%, transparent)",
  },
  ".cm-cursor": {
    borderLeftColor: "var(--color-foreground, #1e293b)",
  },
  ".cm-selectionBackground": {
    backgroundColor: "color-mix(in srgb, var(--color-primary, #3b82f6) 15%, transparent) !important",
  },
  "&.cm-focused .cm-selectionBackground": {
    backgroundColor: "color-mix(in srgb, var(--color-primary, #3b82f6) 20%, transparent) !important",
  },
  ".cm-matchingBracket": {
    backgroundColor: "color-mix(in srgb, var(--color-primary, #3b82f6) 15%, transparent)",
    outline: "1px solid color-mix(in srgb, var(--color-primary, #3b82f6) 30%, transparent)",
  },
});

export interface VfsCodeEditorProps {
  content: string;
  filePath: string;
  readOnly?: boolean;
  onSave?: (content: string) => void | Promise<void>;
  className?: string;
}

function getLanguageExtension(filePath: string): Extension | null {
  const ext = filePath.split(".").pop()?.toLowerCase();
  switch (ext) {
    case "js":
    case "jsx":
    case "mjs":
      return javascript({ jsx: true });
    case "ts":
    case "tsx":
      return javascript({ jsx: true, typescript: true });
    case "json":
      return json();
    case "css":
      return css();
    case "html":
    case "htm":
      return html();
    case "md":
    case "mdx":
      return markdown();
    case "rs":
      return rust();
    case "py":
      return python();
    default:
      return null;
  }
}

function isMarkdownFile(filePath: string): boolean {
  const ext = filePath.split(".").pop()?.toLowerCase();
  return ext === "md" || ext === "mdx";
}

function getDefaultMarkdownViewMode(filePath: string): MarkdownViewMode {
  return isMarkdownFile(filePath) ? "preview" : "edit";
}

export function VfsCodeEditor({
  content,
  filePath,
  readOnly = false,
  onSave,
  className = "",
}: VfsCodeEditorProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const [isDirty, setIsDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [draftContent, setDraftContent] = useState(content);
  const [viewMode, setViewMode] = useState<MarkdownViewMode>(() => getDefaultMarkdownViewMode(filePath));

  const markdownEnabled = isMarkdownFile(filePath);
  const effectiveViewMode = markdownEnabled ? viewMode : "edit";
  const showEditor = effectiveViewMode === "edit" || effectiveViewMode === "split";
  const showPreview = effectiveViewMode === "preview" || effectiveViewMode === "split";
  const previewContent = getMarkdownPreviewContent(draftContent);

  const handleSave = useCallback(async () => {
    if (!viewRef.current || !onSave || readOnly) return;
    const currentContent = viewRef.current.state.doc.toString();
    setSaving(true);
    try {
      await onSave(currentContent);
      setDraftContent(currentContent);
      setIsDirty(false);
    } finally {
      setSaving(false);
    }
  }, [onSave, readOnly]);

  useEffect(() => {
    setDraftContent(content);
    setIsDirty(false);
  }, [content, filePath]);

  useEffect(() => {
    setViewMode(getDefaultMarkdownViewMode(filePath));
  }, [filePath]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const langExt = getLanguageExtension(filePath);
    const extensions: Extension[] = [
      lineNumbers(),
      highlightActiveLine(),
      highlightActiveLineGutter(),
      bracketMatching(),
      highlightSelectionMatches(),
      history(),
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      lightTheme,
      keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap]),
      EditorView.lineWrapping,
      EditorState.readOnly.of(readOnly),
    ];

    if (langExt) extensions.push(langExt);

    if (!readOnly) {
      extensions.push(
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            setDraftContent(update.state.doc.toString());
            setIsDirty(true);
          }
        }),
      );
    }

    const state = EditorState.create({ doc: content, extensions });
    const view = new EditorView({ state, parent: container });
    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
      setIsDirty(false);
    };
  }, [content, filePath, readOnly]);

  return (
    <div className={`flex h-full flex-col ${className}`}>
      {/* 工具栏 */}
      <div className="flex shrink-0 items-center justify-between border-b border-border/50 bg-secondary/10 px-3 py-1">
        <span className="min-w-0 truncate font-mono text-[11px] text-muted-foreground/80">
          {filePath}
        </span>
        <div className="flex shrink-0 items-center gap-2">
          {markdownEnabled && (
            <MarkdownModeControl value={viewMode} onChange={setViewMode} />
          )}
          {isDirty && (
            <span className="text-[10px] text-warning">已修改</span>
          )}
          {!readOnly && onSave && (
            <button
              type="button"
              onClick={() => void handleSave()}
              disabled={saving || !isDirty}
              className="rounded-[4px] border border-success/30 bg-success/10 px-1.5 py-0.5 text-[10px] text-success transition-colors hover:bg-success/20 disabled:opacity-50"
            >
              {saving ? "保存中…" : "保存"}
            </button>
          )}
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-hidden">
        <div className="flex h-full min-h-0">
          {/* 编辑器容器 — CodeMirror 管理自己的滚动 */}
          <div
            className={`${showEditor ? "flex" : "hidden"} min-h-0 min-w-0 flex-col ${
              showPreview ? "w-1/2 border-r border-border/50" : "w-full"
            }`}
          >
            <div ref={containerRef} className="min-h-0 flex-1" />
          </div>

          {markdownEnabled && showPreview && (
            <div className={`${showEditor ? "w-1/2" : "w-full"} min-h-0 min-w-0 overflow-y-auto bg-background`}>
              <div className="mx-auto min-h-full w-full max-w-4xl px-5 py-4">
                {previewContent.trim() ? (
                  <LazyMarkdownRenderer content={previewContent} />
                ) : (
                  <div className="flex min-h-40 items-center justify-center rounded-[6px] border border-dashed border-border bg-secondary/20 px-4 text-center text-xs text-muted-foreground">
                    空 Markdown 文档
                  </div>
                )}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function getMarkdownPreviewContent(content: string): string {
  const lines = content.split(/\r?\n/);
  if (lines[0]?.trim() !== "---") return content;

  const endIndex = lines.findIndex((line, index) => index > 0 && line.trim() === "---");
  if (endIndex < 0) return content;

  return lines.slice(endIndex + 1).join("\n").trimStart();
}

function MarkdownModeControl({
  value,
  onChange,
}: {
  value: MarkdownViewMode;
  onChange: (value: MarkdownViewMode) => void;
}) {
  return (
    <div className="inline-flex h-6 items-center overflow-hidden rounded-[6px] border border-border bg-background text-[10px]">
      <MarkdownModeButton value="edit" current={value} onChange={onChange}>
        编辑
      </MarkdownModeButton>
      <MarkdownModeButton value="preview" current={value} onChange={onChange}>
        预览
      </MarkdownModeButton>
      <MarkdownModeButton value="split" current={value} onChange={onChange}>
        分栏
      </MarkdownModeButton>
    </div>
  );
}

function MarkdownModeButton({
  children,
  value,
  current,
  onChange,
}: {
  children: string;
  value: MarkdownViewMode;
  current: MarkdownViewMode;
  onChange: (value: MarkdownViewMode) => void;
}) {
  const selected = value === current;
  return (
    <button
      type="button"
      aria-pressed={selected}
      onClick={() => onChange(value)}
      className={`h-full min-w-10 px-2 transition-colors ${
        selected
          ? "bg-primary text-primary-foreground"
          : "text-muted-foreground hover:bg-secondary/70 hover:text-foreground"
      }`}
    >
      {children}
    </button>
  );
}

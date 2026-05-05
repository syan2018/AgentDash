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

const lightTheme = EditorView.theme({
  "&": {
    backgroundColor: "var(--color-background, #ffffff)",
    color: "var(--color-foreground, #1e293b)",
    fontSize: "12px",
    fontFamily: "ui-monospace, 'Cascadia Code', 'Fira Code', 'JetBrains Mono', Menlo, Consolas, monospace",
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
  onSave?: (content: string) => void;
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

  const handleSave = useCallback(async () => {
    if (!viewRef.current || !onSave || readOnly) return;
    const currentContent = viewRef.current.state.doc.toString();
    setSaving(true);
    try {
      onSave(currentContent);
      setIsDirty(false);
    } finally {
      setSaving(false);
    }
  }, [onSave, readOnly]);

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
          if (update.docChanged) setIsDirty(true);
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
          {isDirty && (
            <span className="text-[10px] text-amber-600">已修改</span>
          )}
          {!readOnly && onSave && (
            <button
              type="button"
              onClick={() => void handleSave()}
              disabled={saving || !isDirty}
              className="rounded-[4px] border border-emerald-500/30 bg-emerald-500/10 px-1.5 py-0.5 text-[10px] text-emerald-600 transition-colors hover:bg-emerald-500/20 disabled:opacity-50"
            >
              {saving ? "保存中…" : "保存"}
            </button>
          )}
        </div>
      </div>
      {/* 编辑器容器 */}
      <div ref={containerRef} className="min-h-0 flex-1 overflow-hidden" />
    </div>
  );
}

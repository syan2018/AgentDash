import { useRef, useCallback, useState, forwardRef, useImperativeHandle } from "react";
import type { FileEntry } from "../../services/workspaceFiles";
import {
  FILE_PILL_BADGE_CLASS,
  FILE_PILL_CLASS,
  FILE_PILL_LABEL_CLASS,
  getDisplayFileName,
  getFileKindLabel,
  toFileUri,
} from "./fileReferenceUi";

export interface RichInputRef {
  getValue: () => string;
  setValue: (value: string) => void;
  focus: () => void;
  saveSelection: () => void;
  insertFileReference: (file: FileEntry) => void;
}

interface RichInputProps {
  initialValue?: string;
  placeholder?: string;
  onChange?: (value: string) => void;
  onKeyDown?: (e: React.KeyboardEvent) => void;
  onAtTrigger?: (query: string) => void;
  onFileReferenceRemoved?: (relPath: string) => void;
  disabled?: boolean;
}

function escapeHtmlAttr(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

const PILL_CLASS = FILE_PILL_CLASS;

// 将普通文本中的 @path 和 <file:path> 标记转换为可视化药丸
function parseContentToHtml(text: string): string {
  if (!text) return "";

  // 转义 HTML 特殊字符
  let html = text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/\n/g, "<br>");

  // 匹配 <file:path> 标记 - 渲染为药丸
  html = html.replace(
    /&lt;file:([^>]+)&gt;/g,
    (_, path) => {
      const fileName = getDisplayFileName(path);
      const safePath = escapeHtmlAttr(path);
      const safeTitle = escapeHtmlAttr(toFileUri(path));
      return `<span class="${PILL_CLASS}" contenteditable="false" data-file-ref="${safePath}" title="${safeTitle}"><span class="${FILE_PILL_BADGE_CLASS}">${getFileKindLabel(path)}</span><span class="${FILE_PILL_LABEL_CLASS}">${fileName}</span></span>`;
    }
  );

  // 匹配 @path 格式（如果后面没有紧跟文字，则也渲染为药丸）
  // 但在输入过程中保留 @ 触发器的行为
  html = html.replace(
    /@([\w\-./]+)(?=[\s<]|$)/g,
    (match, path) => {
      // 如果这是正在被编辑的 @ 触发器，保持原样
      if (path.length < 1) return match;
      const fileName = getDisplayFileName(path);
      const safePath = escapeHtmlAttr(path);
      const safeTitle = escapeHtmlAttr(toFileUri(path));
      return `<span class="${PILL_CLASS}" contenteditable="false" data-file-ref="${safePath}" title="${safeTitle}"><span class="${FILE_PILL_BADGE_CLASS}">${getFileKindLabel(path)}</span><span class="${FILE_PILL_LABEL_CLASS}">${fileName}</span></span>`;
    }
  );

  return html;
}

// 从 DOM 提取纯文本，保留标记语法
function extractContentFromElement(element: HTMLElement): string {
  const clone = element.cloneNode(true) as HTMLElement;

  // 将药丸 span 替换回 <file:path> 标记
  const pills = clone.querySelectorAll("[data-file-ref]");
  pills.forEach((pill) => {
    const path = pill.getAttribute("data-file-ref");
    if (path) {
      pill.replaceWith(document.createTextNode(`<file:${path}>`));
    }
  });

  // 获取文本内容，处理 <br>
  let text = "";
  const traverse = (node: Node) => {
    if (node.nodeType === Node.TEXT_NODE) {
      text += node.textContent;
    } else if (node.nodeType === Node.ELEMENT_NODE) {
      const el = node as HTMLElement;
      if (el.tagName === "BR") {
        text += "\n";
      } else if (el.tagName === "DIV") {
        // contentEditable 有时会生成 div
        if (text && !text.endsWith("\n")) {
          text += "\n";
        }
        el.childNodes.forEach(traverse);
      } else {
        el.childNodes.forEach(traverse);
      }
    }
  };

  clone.childNodes.forEach(traverse);
  return text;
}

// 获取当前光标位置（用于 @ 触发器）
function getCursorPosition(): { offset: number; textBefore: string } | null {
  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0) return null;

  const range = selection.getRangeAt(0);
  const preRange = range.cloneRange();
  preRange.selectNodeContents(range.startContainer);
  preRange.setEnd(range.startContainer, range.startOffset);

  const offset = preRange.toString().length;
  const textBefore = preRange.toString();

  return { offset, textBefore };
}

export const RichInput = forwardRef<RichInputRef, RichInputProps>(
  (
    {
      initialValue = "",
      placeholder,
      onChange,
      onKeyDown,
      onAtTrigger,
      onFileReferenceRemoved,
      disabled,
    },
    ref,
  ) => {
    const contentRef = useRef<HTMLDivElement>(null);
    const [isEmpty, setIsEmpty] = useState(!initialValue);
    const [isFocused, setIsFocused] = useState(false);
    const isComposingRef = useRef(false);
    const lastValueRef = useRef(initialValue);
    const savedRangeRef = useRef<Range | null>(null);
    const didInitRef = useRef(false);
    const lastPillPathsRef = useRef<Set<string>>(new Set());

    const getPillPaths = useCallback((): Set<string> => {
      const el = contentRef.current;
      if (!el) return new Set();
      const nodes = el.querySelectorAll("[data-file-ref]");
      const s = new Set<string>();
      nodes.forEach((n) => {
        const p = (n as HTMLElement).getAttribute("data-file-ref");
        if (p) s.add(p);
      });
      return s;
    }, []);

    const emitRemovedPills = useCallback(
      (prev: Set<string>, next: Set<string>) => {
        if (!onFileReferenceRemoved) return;
        prev.forEach((p) => {
          if (!next.has(p)) onFileReferenceRemoved(p);
        });
      },
      [onFileReferenceRemoved],
    );

    const updateValue = useCallback(() => {
      if (!contentRef.current) return;

      const prevPills = lastPillPathsRef.current;
      const text = extractContentFromElement(contentRef.current);
      lastValueRef.current = text;
      setIsEmpty(!text.trim());
      onChange?.(text);

      const nextPills = getPillPaths();
      emitRemovedPills(prevPills, nextPills);
      lastPillPathsRef.current = nextPills;
    }, [emitRemovedPills, getPillPaths, onChange]);

    const handleInput = useCallback(() => {
      if (isComposingRef.current) return;
      updateValue();
    }, [updateValue]);

    const handleCompositionStart = useCallback(() => {
      isComposingRef.current = true;
    }, []);

    const handleCompositionEnd = useCallback(() => {
      isComposingRef.current = false;
      updateValue();
    }, [updateValue]);

    const handleKeyDownInternal = useCallback(
      (e: React.KeyboardEvent) => {
        onKeyDown?.(e);

        // 处理 @ 触发器
        if (e.key === "@" && !isComposingRef.current) {
          requestAnimationFrame(() => {
            const pos = getCursorPosition();
            if (pos) {
              onAtTrigger?.("");
            }
          });
        }
      },
      [onKeyDown, onAtTrigger]
    );

    // 监听输入变化以检测 @ 触发器
    const handleKeyUp = useCallback(() => {
      if (isComposingRef.current) return;

      const pos = getCursorPosition();
      if (!pos) return;

      // 检查是否在 @ 后面输入
      const match = pos.textBefore.match(/@([^\s@<>]*)$/);
      if (match) {
        onAtTrigger?.(match[1]);
      }
    }, [onAtTrigger]);

    // 点击药丸时删除它
    const handleClick = useCallback((e: React.MouseEvent) => {
      const target = e.target as HTMLElement;
      const pill = target.closest("[data-file-ref]") as HTMLElement | null;
      if (pill) {
        e.preventDefault();
        e.stopPropagation();

        // 创建 range 选中该药丸
        const range = document.createRange();
        range.selectNode(pill);
        const selection = window.getSelection();
        if (selection) {
          selection.removeAllRanges();
          selection.addRange(range);
          // 删除选中的内容
          range.deleteContents();
          updateValue();
        }
      }
    }, [updateValue]);

    useImperativeHandle(ref, () => ({
      getValue: () => {
        if (!contentRef.current) return "";
        return extractContentFromElement(contentRef.current);
      },
      setValue: (value: string) => {
        if (contentRef.current) {
          const prevPills = getPillPaths();
          const html = parseContentToHtml(value);
          contentRef.current.innerHTML = html;
          lastValueRef.current = value;
          setIsEmpty(!value.trim());
          onChange?.(value);

          const nextPills = getPillPaths();
          emitRemovedPills(prevPills, nextPills);
          lastPillPathsRef.current = nextPills;
        }
      },
      focus: () => {
        contentRef.current?.focus();
      },
      saveSelection: () => {
        if (!contentRef.current) return;
        const selection = window.getSelection();
        if (!selection || selection.rangeCount === 0) return;
        const range = selection.getRangeAt(0);
        if (!contentRef.current.contains(range.startContainer)) return;
        savedRangeRef.current = range.cloneRange();
      },
      insertFileReference: (file: FileEntry) => {
        if (!contentRef.current) return;

        // 确保 RichInput 获得焦点
        contentRef.current.focus();

        const selection = window.getSelection();
        if (!selection) {
          // 极端情况下（页面未聚焦等）可能拿不到 selection：回退到追加文本标记。
          const current = extractContentFromElement(contentRef.current);
          const needsSpace = current.length > 0 && !/[ \n]$/.test(current);
          const next = `${current}${needsSpace ? " " : ""}<file:${file.relPath}> `;
          const html = parseContentToHtml(next);
          contentRef.current.innerHTML = html;
          lastValueRef.current = next;
          setIsEmpty(!next.trim());
          onChange?.(next);
          return;
        }

        // 优先使用保存的 range（如果可用），否则使用当前 selection
        let range: Range;
        if (savedRangeRef.current && contentRef.current.contains(savedRangeRef.current.startContainer)) {
          range = savedRangeRef.current.cloneRange();
          selection.removeAllRanges();
          selection.addRange(range);
        } else if (selection.rangeCount > 0) {
          range = selection.getRangeAt(0);
        } else {
          // 如果没有可用的 range，将光标放在最后
          range = document.createRange();
          range.selectNodeContents(contentRef.current);
          range.collapse(false);
          selection.addRange(range);
        }

        // 查找并替换 @query
        let textNode: Text | null = null;
        let textOffset = 0;

        // 找到包含光标的文本节点
        if (range.startContainer.nodeType === Node.TEXT_NODE) {
          textNode = range.startContainer as Text;
          textOffset = range.startOffset;
        } else {
          // 如果不在文本节点上，尝试在光标位置前找一个文本节点
          const treeWalker = document.createTreeWalker(
            contentRef.current,
            NodeFilter.SHOW_TEXT,
            null
          );
          let lastTextNode: Text | null = null;
          while (treeWalker.nextNode()) {
            lastTextNode = treeWalker.currentNode as Text;
          }
          if (lastTextNode) {
            textNode = lastTextNode;
            textOffset = lastTextNode.length;
          } else {
            // 创建一个新的文本节点
            textNode = document.createTextNode("");
            range.insertNode(textNode);
            textOffset = 0;
          }
        }

        const textContent = textNode.textContent || "";
        const beforeText = textContent.slice(0, textOffset);
        const afterText = textContent.slice(textOffset);

        const atMatch = beforeText.match(/@([^\s@<>]*)$/);

        if (atMatch) {
          // 删除 @query
          const newBeforeText = beforeText.slice(0, atMatch.index);
          textNode.textContent = newBeforeText + afterText;

          // 设置光标位置
          const newRange = document.createRange();
          newRange.setStart(textNode, newBeforeText.length);
          newRange.collapse(true);
          selection.removeAllRanges();
          selection.addRange(newRange);
          range = newRange;
        }

        // 插入药丸
        const pill = document.createElement("span");
        pill.className = PILL_CLASS;
        pill.contentEditable = "false";
        pill.setAttribute("data-file-ref", file.relPath);
        pill.title = toFileUri(file.relPath);

        const badge = document.createElement("span");
        badge.className = FILE_PILL_BADGE_CLASS;
        badge.textContent = getFileKindLabel(file.relPath);

        const name = document.createElement("span");
        name.className = FILE_PILL_LABEL_CLASS;
        name.textContent = getDisplayFileName(file.relPath);

        pill.appendChild(badge);
        pill.appendChild(name);

        // 插入空格
        const space = document.createTextNode(" ");

        // 在当前 range 位置插入
        range.deleteContents();
        range.insertNode(space);
        range.setStartBefore(space);
        range.insertNode(pill);
        range.setStartAfter(space);
        range.collapse(true);

        selection.removeAllRanges();
        selection.addRange(range);

        // 清除保存的 range
        savedRangeRef.current = null;

        updateValue();
      },
    }));

    return (
      <div
        className={`relative rounded-[12px] border border-border bg-background transition-all ${
          isFocused ? "border-primary/30 ring-1 ring-ring/40" : ""
        } ${disabled ? "opacity-50" : ""}`}
      >
        {/* Placeholder */}
        {isEmpty && !isFocused && (
          <div className="pointer-events-none absolute left-4 top-3 text-sm text-muted-foreground">
            {placeholder}
          </div>
        )}

        {/* Content Editable */}
        <div
          ref={(node) => {
            contentRef.current = node;
            if (!node) return;
            if (didInitRef.current) return;
            // Initialize innerHTML once. After that, treat as an uncontrolled contentEditable
            // and mutate via user input / imperative methods. Avoid re-render wiping DOM.
            didInitRef.current = true;
            const html = parseContentToHtml(initialValue);
            node.innerHTML = html;
            lastValueRef.current = initialValue;
            setIsEmpty(!initialValue.trim());
            lastPillPathsRef.current = getPillPaths();
          }}
          contentEditable={!disabled}
          onInput={handleInput}
          onCompositionStart={handleCompositionStart}
          onCompositionEnd={handleCompositionEnd}
          onKeyDown={handleKeyDownInternal}
          onKeyUp={handleKeyUp}
          onClick={handleClick}
          onFocus={() => {
            setIsFocused(true);
            // 恢复之前保存的 selection
            if (savedRangeRef.current) {
              const selection = window.getSelection();
              if (selection) {
                selection.removeAllRanges();
                selection.addRange(savedRangeRef.current);
              }
            }
          }}
          onBlur={() => {
            setIsFocused(false);
            // 保存当前的 selection
            const selection = window.getSelection();
            if (selection && selection.rangeCount > 0) {
              savedRangeRef.current = selection.getRangeAt(0).cloneRange();
            }
          }}
          className="min-h-[88px] w-full px-4 py-3 text-sm leading-7 outline-none"
          style={{ whiteSpace: "pre-wrap" }}
        />
      </div>
    );
  }
);

RichInput.displayName = "RichInput";

export default RichInput;

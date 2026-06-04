/**
 * 「+」菜单按钮 — Composer 底部工具栏入口
 *
 * MVP: 「添加文件/图片」→ 拉起系统文件选择器。
 */

import { useCallback, useRef, useState, useEffect } from "react";

interface ComposerPlusMenuProps {
  disabled?: boolean;
  onSelectFiles: (files: FileList) => void;
}

export function ComposerPlusMenu({ disabled, onSelectFiles }: ComposerPlusMenuProps) {
  const [open, setOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    function handleClick(e: MouseEvent) {
      if (
        menuRef.current &&
        !menuRef.current.contains(e.target as Node) &&
        buttonRef.current &&
        !buttonRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  const handleFileClick = useCallback(() => {
    setOpen(false);
    fileInputRef.current?.click();
  }, []);

  const handleFileChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const files = e.target.files;
      if (files && files.length > 0) {
        onSelectFiles(files);
      }
      e.target.value = "";
    },
    [onSelectFiles],
  );

  return (
    <div className="relative">
      <button
        ref={buttonRef}
        type="button"
        disabled={disabled}
        onClick={() => setOpen((v) => !v)}
        className="flex h-8 w-8 items-center justify-center rounded-[50%] text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-foreground disabled:opacity-30"
        title="添加附件"
      >
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
          <path d="M8 3V13M3 8H13" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
        </svg>
      </button>

      {open && (
        <div
          ref={menuRef}
          className="absolute bottom-full left-0 z-50 mb-2 w-44 rounded-[12px] border border-border/60 bg-popover p-1 shadow-lg"
        >
          <button
            type="button"
            onClick={handleFileClick}
            className="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-xs text-foreground transition-colors hover:bg-muted"
          >
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" className="shrink-0 text-muted-foreground">
              <path d="M14 10V13C14 13.5523 13.5523 14 13 14H3C2.44772 14 2 13.5523 2 13V10M11 5L8 2M8 2L5 5M8 2V10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
            添加文件 / 图片
          </button>
        </div>
      )}

      <input
        ref={fileInputRef}
        type="file"
        multiple
        accept="image/*,.txt,.md,.json,.yaml,.yml,.toml,.xml,.csv,.ts,.tsx,.js,.jsx,.py,.rs,.go,.java,.c,.cpp,.h,.hpp,.cs,.rb,.php,.swift,.kt,.sh,.bash,.sql,.html,.css,.scss,.less"
        onChange={handleFileChange}
        className="hidden"
      />
    </div>
  );
}

/**
 * CreateSkillDialog — Skill 创建/导入统一入口。
 *
 * 提供三种方式：
 * - Manual：手动从零创建（回调父组件打开 SkillEditorDialog）
 * - URL Import：从 GitHub / ClawHub / skills.sh 远端导入
 * - Workspace Scan：装载内嵌 Skill + 上传 ZIP/目录
 *
 * 参照 multica create-skill-dialog 的 method chooser 分步体验。
 */

import { useCallback, useRef, useState } from "react";

import {
  bootstrapSkillAssets,
  importRemoteSkillAsset,
  uploadSkillAssets,
} from "../../../services/skillAsset";

// ─── Types ───────────────────────────────────────────────

type Method = "chooser" | "manual" | "url" | "workspace";

type DetectedSource = "github" | "clawhub" | "skills_sh" | null;

const METHOD_META: Record<
  Exclude<Method, "chooser">,
  { title: string; desc: string }
> = {
  manual: { title: "手动创建", desc: "从零填写 Skill 名称、描述和内容" },
  url: { title: "远端导入", desc: "从 GitHub 等远端 URL 导入已有 Skill" },
  workspace: { title: "工作区导入", desc: "装载内嵌 Skill 或上传本地 Skill 包" },
};

// ─── Source detection ────────────────────────────────────

function detectUrlSource(url: string): DetectedSource {
  const lower = url.trim().toLowerCase();
  if (lower.includes("github.com")) return "github";
  if (lower.includes("clawhub.ai") || lower.includes("clawhub.com")) return "clawhub";
  if (lower.includes("skills.sh")) return "skills_sh";
  return null;
}

const SOURCE_CARDS: Array<{
  key: DetectedSource & string;
  label: string;
  host: string;
  browseUrl: string;
}> = [
  {
    key: "github",
    label: "GitHub",
    host: "github.com",
    browseUrl: "https://github.com/topics/agent-skill",
  },
  {
    key: "clawhub",
    label: "ClawHub",
    host: "clawhub.ai",
    browseUrl: "https://clawhub.ai",
  },
  {
    key: "skills_sh",
    label: "Skills.sh",
    host: "skills.sh",
    browseUrl: "https://skills.sh",
  },
];

// ─── Props ───────────────────────────────────────────────

interface CreateSkillDialogProps {
  projectId: string;
  onClose: () => void;
  /** 创建/导入成功后的回调，message 用于 toast 展示 */
  onCreated: (message: string) => void;
  /** 用户选择"手动创建"时的回调（由父组件打开 SkillEditorDialog） */
  onOpenManualCreate: () => void;
}

// ─── Root Dialog ─────────────────────────────────────────

export function CreateSkillDialog({
  projectId,
  onClose,
  onCreated,
  onOpenManualCreate,
}: CreateSkillDialogProps) {
  const [method, setMethod] = useState<Method>("chooser");

  const handleManualChoose = useCallback(() => {
    onClose();
    onOpenManualCreate();
  }, [onClose, onOpenManualCreate]);

  const wide = method === "workspace";

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6"
      onClick={onClose}
    >
      <div
        className={`flex max-h-[88vh] flex-col overflow-hidden rounded-[8px] border border-border bg-background shadow-xl transition-[width] ${
          wide ? "w-[720px]" : "w-[560px]"
        } max-w-full`}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <header className="flex items-center justify-between border-b border-border px-5 py-4">
          <div className="flex items-center gap-2">
            {method !== "chooser" && (
              <button
                type="button"
                onClick={() => setMethod("chooser")}
                className="inline-flex h-7 w-7 items-center justify-center rounded-[6px] text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
                aria-label="返回"
              >
                <ArrowLeftIcon />
              </button>
            )}
            <div>
              <h3 className="text-sm font-semibold text-foreground">
                {method === "chooser"
                  ? "新建 Skill"
                  : METHOD_META[method].title}
              </h3>
              <p className="mt-0.5 text-xs text-muted-foreground">
                {method === "chooser"
                  ? "选择创建或导入方式"
                  : METHOD_META[method].desc}
              </p>
            </div>
          </div>
          <button type="button" onClick={onClose} className="agentdash-button-secondary">
            关闭
          </button>
        </header>

        {/* Body */}
        {method === "chooser" && (
          <MethodChooser onChoose={setMethod} onManual={handleManualChoose} />
        )}
        {method === "url" && (
          <UrlImportForm
            projectId={projectId}
            onCreated={onCreated}
            onCancel={() => setMethod("chooser")}
          />
        )}
        {method === "workspace" && (
          <WorkspaceScanPanel
            projectId={projectId}
            onCreated={onCreated}
            onCancel={() => setMethod("chooser")}
          />
        )}
      </div>
    </div>
  );
}

// ─── Method Chooser ──────────────────────────────────────

function MethodChooser({
  onChoose,
  onManual,
}: {
  onChoose: (m: Method) => void;
  onManual: () => void;
}) {
  const cards: Array<{
    method: Method;
    icon: () => JSX.Element;
    title: string;
    desc: string;
    onClick: () => void;
  }> = [
    {
      method: "manual",
      icon: PencilIcon,
      title: "手动创建",
      desc: "从零填写 name、description 和 SKILL.md 内容，适合自定义 Skill",
      onClick: onManual,
    },
    {
      method: "url",
      icon: DownloadIcon,
      title: "远端导入",
      desc: "输入 GitHub URL 自动下载 SKILL.md 和支持文件，快速引入外部 Skill",
      onClick: () => onChoose("url"),
    },
    {
      method: "workspace",
      icon: FolderIcon,
      title: "工作区导入",
      desc: "装载项目内嵌 Skill 或从本地 ZIP / 目录上传 Skill 包",
      onClick: () => onChoose("workspace"),
    },
  ];

  return (
    <div className="grid gap-3 p-5 sm:grid-cols-3">
      {cards.map((card) => (
        <button
          key={card.method}
          type="button"
          onClick={card.onClick}
          className="group flex flex-col items-start gap-3 rounded-[8px] border border-border bg-background p-4 text-left transition-colors hover:border-primary/40 hover:bg-secondary/30"
        >
          <div className="inline-flex h-9 w-9 items-center justify-center rounded-[8px] border border-border bg-secondary/50 text-muted-foreground transition-colors group-hover:border-primary/30 group-hover:text-foreground">
            <card.icon />
          </div>
          <div>
            <p className="text-sm font-medium text-foreground">{card.title}</p>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              {card.desc}
            </p>
          </div>
          <ChevronRightIcon />
        </button>
      ))}
    </div>
  );
}

// ─── URL Import Form ─────────────────────────────────────

function UrlImportForm({
  projectId,
  onCreated,
  onCancel,
}: {
  projectId: string;
  onCreated: (message: string) => void;
  onCancel: () => void;
}) {
  const [url, setUrl] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const source = detectUrlSource(url);

  const submit = useCallback(async () => {
    const trimmed = url.trim();
    if (!trimmed) return;
    setLoading(true);
    setError("");
    try {
      const imported = await importRemoteSkillAsset(projectId, { url: trimmed });
      onCreated(`已导入远端 Skill：${imported.key}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "导入远端 Skill 失败");
      setLoading(false);
    }
  }, [url, projectId, onCreated]);

  const loadingLabel = (() => {
    if (!loading) return "导入";
    if (source === "github") return "正在从 GitHub 导入…";
    if (source === "clawhub") return "正在从 ClawHub 导入…";
    if (source === "skills_sh") return "正在从 Skills.sh 导入…";
    return "正在导入…";
  })();

  return (
    <>
      <div className="flex-1 space-y-4 overflow-y-auto p-5">
        {/* URL Input */}
        <label className="block space-y-1.5">
          <span className="agentdash-form-label">Skill URL</span>
          <input
            value={url}
            onChange={(e) => {
              setUrl(e.target.value);
              setError("");
            }}
            placeholder="https://github.com/org/repo/tree/main/skills/my-skill"
            className="agentdash-form-input font-mono text-sm"
            onKeyDown={(e) => {
              if (e.key === "Enter") void submit();
            }}
          />
        </label>

        {/* Source Cards */}
        <div className="space-y-2">
          <p className="text-xs font-medium text-muted-foreground">支持的来源</p>
          <div className="grid gap-2 sm:grid-cols-3">
            {SOURCE_CARDS.map((card) => (
              <div
                key={card.key}
                className={`rounded-[8px] border p-3 transition-colors ${
                  source === card.key
                    ? "border-primary/40 bg-primary/5"
                    : "border-border bg-background"
                }`}
              >
                <span className="text-xs font-medium text-foreground">{card.label}</span>
                <a
                  href={card.browseUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="mt-1 block truncate font-mono text-[11px] text-primary/70 underline decoration-primary/30 underline-offset-2 hover:text-primary hover:decoration-primary/60"
                >
                  {card.host}
                </a>
              </div>
            ))}
          </div>
        </div>

        {/* Error */}
        {error && (
          <div className="flex items-start gap-2 rounded-[8px] border border-destructive/30 bg-destructive/5 px-3 py-2.5">
            <AlertIcon />
            <p className="text-xs leading-5 text-destructive">{error}</p>
          </div>
        )}
      </div>

      {/* Footer */}
      <footer className="flex justify-end gap-2 border-t border-border px-5 py-4">
        <button type="button" onClick={onCancel} className="agentdash-button-secondary">
          返回
        </button>
        <button
          type="button"
          onClick={() => void submit()}
          disabled={loading || !url.trim()}
          className="agentdash-button-primary"
        >
          {loading ? (
            <span className="flex items-center gap-1.5">
              <SpinnerIcon />
              {loadingLabel}
            </span>
          ) : (
            <span className="flex items-center gap-1.5">
              <DownloadIcon />
              导入
            </span>
          )}
        </button>
      </footer>
    </>
  );
}

// ─── Workspace Scan Panel ────────────────────────────────

function WorkspaceScanPanel({
  projectId,
  onCreated,
  onCancel,
}: {
  projectId: string;
  onCreated: (message: string) => void;
  onCancel: () => void;
}) {
  const zipInputRef = useRef<HTMLInputElement | null>(null);
  const directoryInputRef = useRef<HTMLInputElement | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const handleBootstrap = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const result = await bootstrapSkillAssets(projectId);
      if (result.length === 0) {
        setError("未发现新的内嵌 Skill（可能已全部装载）");
        setLoading(false);
      } else {
        onCreated(`已装载 ${result.length} 个内嵌 Skill`);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "装载内嵌 Skill 失败");
      setLoading(false);
    }
  }, [projectId, onCreated]);

  const handleUpload = useCallback(
    async (fileList: FileList | null) => {
      if (!fileList || fileList.length === 0) return;
      setLoading(true);
      setError("");
      try {
        const uploaded = await uploadSkillAssets(projectId, Array.from(fileList));
        onCreated(`已导入 ${uploaded.length} 个 Skill`);
      } catch (err) {
        setError(err instanceof Error ? err.message : "上传 Skill 失败");
        setLoading(false);
      } finally {
        if (zipInputRef.current) zipInputRef.current.value = "";
        if (directoryInputRef.current) directoryInputRef.current.value = "";
      }
    },
    [projectId, onCreated],
  );

  return (
    <>
      <div className="flex-1 space-y-4 overflow-y-auto p-5">
        {/* Bootstrap Section */}
        <section className="rounded-[8px] border border-border bg-background p-4">
          <div className="flex items-start gap-3">
            <div className="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-[8px] border border-border bg-secondary/50 text-muted-foreground">
              <BoxIcon />
            </div>
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium text-foreground">装载内嵌 Skill</p>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">
                从项目内嵌定义中导入预置的 Skill 资产，包括系统和模板 Skill。
              </p>
              <button
                type="button"
                onClick={() => void handleBootstrap()}
                disabled={loading}
                className="agentdash-button-secondary mt-3"
              >
                {loading ? "装载中…" : "装载内嵌 Skill"}
              </button>
            </div>
          </div>
        </section>

        {/* Upload Section */}
        <section className="rounded-[8px] border border-border bg-background p-4">
          <div className="flex items-start gap-3">
            <div className="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-[8px] border border-border bg-secondary/50 text-muted-foreground">
              <UploadIcon />
            </div>
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium text-foreground">上传本地 Skill</p>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">
                从本地文件系统上传 Skill 包。支持包含 SKILL.md 的目录或 ZIP 压缩包。
              </p>
              <div className="mt-3 flex flex-wrap gap-2">
                <input
                  ref={zipInputRef}
                  type="file"
                  accept=".zip"
                  className="hidden"
                  onChange={(e) => void handleUpload(e.currentTarget.files)}
                />
                <input
                  ref={directoryInputRef}
                  type="file"
                  multiple
                  className="hidden"
                  onChange={(e) => void handleUpload(e.currentTarget.files)}
                  {...{ webkitdirectory: "true", directory: "true" }}
                />
                <button
                  type="button"
                  onClick={() => directoryInputRef.current?.click()}
                  disabled={loading}
                  className="agentdash-button-secondary"
                >
                  上传目录
                </button>
                <button
                  type="button"
                  onClick={() => zipInputRef.current?.click()}
                  disabled={loading}
                  className="agentdash-button-secondary"
                >
                  上传 ZIP
                </button>
              </div>
            </div>
          </div>
        </section>

        {/* Error */}
        {error && (
          <div className="flex items-start gap-2 rounded-[8px] border border-destructive/30 bg-destructive/5 px-3 py-2.5">
            <AlertIcon />
            <p className="text-xs leading-5 text-destructive">{error}</p>
          </div>
        )}
      </div>

      {/* Footer */}
      <footer className="flex justify-end gap-2 border-t border-border px-5 py-4">
        <button type="button" onClick={onCancel} className="agentdash-button-secondary">
          返回
        </button>
      </footer>
    </>
  );
}

// ─── Icons ───────────────────────────────────────────────

function PencilIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M12 20h9" />
      <path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z" />
    </svg>
  );
}

function DownloadIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
      <polyline points="7 10 12 15 17 10" />
      <line x1="12" y1="15" x2="12" y2="3" />
    </svg>
  );
}

function FolderIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
    </svg>
  );
}

function BoxIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z" />
      <path d="m3.3 7 8.7 5 8.7-5" />
      <path d="M12 22V12" />
    </svg>
  );
}

function UploadIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
      <polyline points="17 8 12 3 7 8" />
      <line x1="12" y1="3" x2="12" y2="15" />
    </svg>
  );
}

function ArrowLeftIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M19 12H5" />
      <polyline points="12 19 5 12 12 5" />
    </svg>
  );
}

function ChevronRightIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" className="ml-auto mt-auto text-muted-foreground/50 transition-colors group-hover:text-foreground/60" aria-hidden="true">
      <polyline points="9 18 15 12 9 6" />
    </svg>
  );
}

function AlertIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="mt-0.5 shrink-0 text-destructive" aria-hidden="true">
      <circle cx="12" cy="12" r="10" />
      <line x1="12" y1="8" x2="12" y2="12" />
      <line x1="12" y1="16" x2="12.01" y2="16" />
    </svg>
  );
}

function SpinnerIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="animate-spin" aria-hidden="true">
      <path d="M21 12a9 9 0 1 1-6.219-8.56" />
    </svg>
  );
}

/**
 * SkillCategoryPanel — Assets 页 Skill 类目。
 *
 * 布局：
 * - 简洁 header：标题 + 来源统计 + 刷新 + 新建按钮
 * - 卡片网格：优化的 origin badge、来源 URL 展示
 * - 新建/导入通过 CreateSkillDialog 分步体验（Manual / URL / Workspace）
 * - 编辑仍使用 SkillEditorDialog（VFS 浏览器模式 + 创建表单模式）
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";

import { useProjectStore } from "../../../stores/projectStore";
import { VfsBrowser, VfsCodeEditor, type VfsBrowserPanelInspectorContext } from "../../vfs";
import {
  buildSkillYamlFrontmatter,
  createEmptySkillAssetDraft,
  createSkillAsset,
  deleteSkillAsset,
  draftFromSkillAsset,
  dtoFilesFromDraft,
  fetchProjectSkillAssets,
  normalizeSkillExtraPath,
  parseSkillMarkdown,
  resetSkillAssetFromBuiltin,
  updateSkillMarkdownFrontmatter,
  updateSkillAsset,
  validateSkillAssetDraft,
  type SkillAssetDraft,
} from "../../../services/skillAsset";
import type { SkillAssetDto } from "../../../types";
import { CreateSkillDialog } from "./CreateSkillDialog";

// ─── Detail mode ─────────────────────────────────────────

type DetailMode =
  | { kind: "closed" }
  | { kind: "create" }
  | { kind: "edit"; assetId: string; originalKey: string };

function cloneDraft(draft: SkillAssetDraft): SkillAssetDraft {
  return { ...draft, files: draft.files.map((f) => ({ ...f })) };
}

// ─── Main Panel ──────────────────────────────────────────

export function SkillCategoryPanel() {
  const currentProjectId = useProjectStore((s) => s.currentProjectId);
  const projects = useProjectStore((s) => s.projects);
  const currentProject = useMemo(
    () => projects.find((p) => p.id === currentProjectId) ?? null,
    [currentProjectId, projects],
  );

  const [skills, setSkills] = useState<SkillAssetDto[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [detail, setDetail] = useState<DetailMode>({ kind: "closed" });
  const [draft, setDraft] = useState<SkillAssetDraft>(() => createEmptySkillAssetDraft());
  const [confirmDelete, setConfirmDelete] = useState<SkillAssetDto | null>(null);
  const [showCreateDialog, setShowCreateDialog] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // ── Data loading ────────────────────────────────────

  const loadSkills = useCallback(async () => {
    if (!currentProjectId) return;
    setIsLoading(true);
    setError(null);
    try {
      setSkills(await fetchProjectSkillAssets(currentProjectId));
    } catch (e) {
      setError(e instanceof Error ? e.message : "加载 Skill 资产失败");
    } finally {
      setIsLoading(false);
    }
  }, [currentProjectId]);

  useEffect(() => {
    void loadSkills();
  }, [loadSkills]);

  useEffect(() => {
    if (!message) return;
    const timer = setTimeout(() => setMessage(null), 4000);
    return () => clearTimeout(timer);
  }, [message]);

  // ── Stats ───────────────────────────────────────────

  const stats = useMemo(() => {
    const counts: Record<string, number> = {};
    for (const s of skills) {
      const tag = s.source === "builtin_seed" ? "builtin" : s.source;
      counts[tag] = (counts[tag] ?? 0) + 1;
    }
    return counts;
  }, [skills]);

  // ── Actions ─────────────────────────────────────────

  const openManualCreate = useCallback(() => {
    setDraft(createEmptySkillAssetDraft());
    setError(null);
    setDetail({ kind: "create" });
  }, []);

  const openEdit = useCallback((skill: SkillAssetDto) => {
    setDraft(cloneDraft(draftFromSkillAsset(skill)));
    setError(null);
    setDetail({ kind: "edit", assetId: skill.id, originalKey: skill.key });
  }, []);

  const handleSaveDraft = useCallback(async () => {
    if (!currentProjectId || detail.kind === "closed") return;
    const normalizedDraft: SkillAssetDraft = {
      ...draft,
      key: draft.key.trim(),
      display_name: draft.display_name.trim() || draft.key.trim(),
      description: draft.description.trim(),
      files: draft.files
        .filter((f) => normalizeSkillExtraPath(f.relative_path))
        .map((f) => ({
          relative_path: normalizeSkillExtraPath(f.relative_path),
          content: f.content,
        })),
    };
    const existingKeys =
      detail.kind === "edit"
        ? skills.map((s) => s.key).filter((k) => k !== detail.originalKey)
        : skills.map((s) => s.key);
    const validation = validateSkillAssetDraft(normalizedDraft, existingKeys);
    if (!validation.ok) {
      setError(validation.message ?? "Skill 表单校验失败");
      return;
    }

    setIsSaving(true);
    setError(null);
    try {
      const files = dtoFilesFromDraft(normalizedDraft);
      if (detail.kind === "create") {
        await createSkillAsset(currentProjectId, {
          key: normalizedDraft.key,
          display_name: normalizedDraft.display_name,
          description: normalizedDraft.description,
          disable_model_invocation: normalizedDraft.disable_model_invocation,
          files,
        });
      } else {
        await updateSkillAsset(currentProjectId, detail.assetId, {
          key: normalizedDraft.key,
          display_name: normalizedDraft.display_name,
          description: normalizedDraft.description,
          disable_model_invocation: normalizedDraft.disable_model_invocation,
          files,
        });
      }
      setMessage(`已保存 Skill：${normalizedDraft.key}`);
      setDetail({ kind: "closed" });
      await loadSkills();
    } catch (e) {
      setError(e instanceof Error ? e.message : "保存 Skill 资产失败");
    } finally {
      setIsSaving(false);
    }
  }, [currentProjectId, detail, draft, loadSkills, skills]);

  const handleDelete = useCallback(async () => {
    if (!currentProjectId || !confirmDelete) return;
    setBusyId(confirmDelete.id);
    setError(null);
    try {
      await deleteSkillAsset(currentProjectId, confirmDelete.id);
      setMessage(`已删除 Skill：${confirmDelete.key}`);
      if (detail.kind === "edit" && detail.assetId === confirmDelete.id) {
        setDetail({ kind: "closed" });
      }
      setConfirmDelete(null);
      await loadSkills();
    } catch (e) {
      setError(e instanceof Error ? e.message : "删除 Skill 资产失败");
    } finally {
      setBusyId(null);
    }
  }, [confirmDelete, currentProjectId, detail, loadSkills]);

  const handleReset = useCallback(
    async (skill: SkillAssetDto) => {
      if (!currentProjectId) return;
      setBusyId(skill.id);
      setError(null);
      try {
        await resetSkillAssetFromBuiltin(currentProjectId, skill.id);
        setMessage(`已恢复内嵌 Skill：${skill.key}`);
        await loadSkills();
      } catch (e) {
        setError(e instanceof Error ? e.message : "恢复内嵌 Skill 失败");
      } finally {
        setBusyId(null);
      }
    },
    [currentProjectId, loadSkills],
  );

  const handleCreateDialogCreated = useCallback(
    (msg: string) => {
      setMessage(msg);
      setShowCreateDialog(false);
      void loadSkills();
    },
    [loadSkills],
  );

  // ── Guard ───────────────────────────────────────────

  if (!currentProjectId || !currentProject) {
    return (
      <div className="flex h-full items-center justify-center p-6">
        <div className="text-center text-sm text-muted-foreground">请选择项目后查看 Skill 资产</div>
      </div>
    );
  }

  // ── Render ──────────────────────────────────────────

  const statsText = Object.entries(stats)
    .map(([tag, count]) => `${count} 个 ${tag}`)
    .join(" · ");

  return (
    <div className="flex h-full flex-col gap-4 p-6">
      {/* ── Header ── */}
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div className="space-y-1">
          <h2 className="text-base font-semibold tracking-tight text-foreground">Skill 资产</h2>
          <p className="text-xs text-muted-foreground">
            {skills.length > 0
              ? `${statsText} · Agent preset 可按 key 装载`
              : "0 个 Skill · Agent preset 可按 key 装载"}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => void loadSkills()}
            disabled={isLoading}
            className="agentdash-button-secondary"
          >
            {isLoading ? "刷新中…" : "刷新"}
          </button>
          <button
            type="button"
            onClick={() => setShowCreateDialog(true)}
            className="agentdash-button-primary"
          >
            新建 Skill
          </button>
        </div>
      </header>

      {/* ── Notices ── */}
      {message && <Notice tone="success" message={message} onClose={() => setMessage(null)} />}
      {error && <Notice tone="danger" message={error} onClose={() => setError(null)} />}

      {/* ── Grid ── */}
      {isLoading ? (
        <div className="rounded-[8px] border border-dashed border-border px-6 py-10 text-center text-sm text-muted-foreground">
          正在加载 Skill 资产…
        </div>
      ) : (
        <SkillGrid
          skills={skills}
          busyId={busyId}
          onEdit={openEdit}
          onDelete={setConfirmDelete}
          onReset={(skill) => void handleReset(skill)}
        />
      )}

      {/* ── CreateSkillDialog ── */}
      {showCreateDialog && (
        <CreateSkillDialog
          projectId={currentProjectId}
          onClose={() => setShowCreateDialog(false)}
          onCreated={handleCreateDialogCreated}
          onOpenManualCreate={openManualCreate}
        />
      )}

      {/* ── Editor Dialog ── */}
      {detail.kind !== "closed" && (
        <SkillEditorDialog
          mode={detail.kind}
          projectId={currentProjectId}
          draft={draft}
          isSaving={isSaving}
          onDraftChange={setDraft}
          onClose={() => {
            setDetail({ kind: "closed" });
            void loadSkills();
          }}
          onSave={() => void handleSaveDraft()}
        />
      )}

      {/* ── Delete Confirm ── */}
      {confirmDelete && (
        <ConfirmDeleteDialog
          skill={confirmDelete}
          busy={busyId === confirmDelete.id}
          onCancel={() => setConfirmDelete(null)}
          onConfirm={() => void handleDelete()}
        />
      )}
    </div>
  );
}

export default SkillCategoryPanel;

// ─── Notice ──────────────────────────────────────────────

function Notice({
  tone,
  message,
  onClose,
}: {
  tone: "success" | "danger";
  message: string;
  onClose: () => void;
}) {
  const cls =
    tone === "success"
      ? "border-emerald-300/30 bg-emerald-500/5 text-emerald-600"
      : "border-destructive/30 bg-destructive/5 text-destructive";
  return (
    <div className={`flex items-center justify-between rounded-[8px] border px-3 py-2 ${cls}`}>
      <p className="text-xs">{message}</p>
      <button type="button" onClick={onClose} className="ml-2 text-xs opacity-70 hover:opacity-100">
        x
      </button>
    </div>
  );
}

// ─── Origin Badge ────────────────────────────────────────

const ORIGIN_STYLE: Record<
  string,
  { label: string; border: string; bg: string; text: string }
> = {
  builtin_seed: {
    label: "builtin",
    border: "border-border",
    bg: "bg-secondary/50",
    text: "text-muted-foreground",
  },
  user: {
    label: "user",
    border: "border-violet-500/30",
    bg: "bg-violet-500/10",
    text: "text-violet-700 dark:text-violet-300",
  },
  github: {
    label: "github",
    border: "border-sky-500/30",
    bg: "bg-sky-500/10",
    text: "text-sky-700 dark:text-sky-300",
  },
  clawhub: {
    label: "clawhub",
    border: "border-emerald-500/30",
    bg: "bg-emerald-500/10",
    text: "text-emerald-700 dark:text-emerald-300",
  },
  skills_sh: {
    label: "skills.sh",
    border: "border-orange-500/30",
    bg: "bg-orange-500/10",
    text: "text-orange-700 dark:text-orange-300",
  },
};

function OriginBadge({ skill }: { skill: SkillAssetDto }) {
  const style = ORIGIN_STYLE[skill.source] ?? ORIGIN_STYLE.user;
  const remoteUrl = skill.remote_source?.url;

  const shortUrl = remoteUrl
    ? remoteUrl
        .replace(/^https?:\/\//, "")
        .replace(/^github\.com\//, "")
        .slice(0, 36)
    : null;

  return (
    <span
      title={remoteUrl ?? undefined}
      className={`inline-flex max-w-[180px] items-center gap-1 truncate rounded-[6px] border px-1.5 py-0.5 text-[10px] ${style.border} ${style.bg} ${style.text}`}
    >
      {style.label}
      {shortUrl && (
        <span className="truncate opacity-70" title={remoteUrl ?? undefined}>
          · {shortUrl}
        </span>
      )}
    </span>
  );
}

// ─── Skill Grid ──────────────────────────────────────────

function SkillGrid({
  skills,
  busyId,
  onEdit,
  onDelete,
  onReset,
}: {
  skills: SkillAssetDto[];
  busyId: string | null;
  onEdit: (skill: SkillAssetDto) => void;
  onDelete: (skill: SkillAssetDto) => void;
  onReset: (skill: SkillAssetDto) => void;
}) {
  if (skills.length === 0) {
    return (
      <div className="rounded-[8px] border border-dashed border-border bg-secondary/20 px-6 py-14 text-center">
        <p className="text-sm text-foreground">暂无 Skill 资产</p>
        <p className="mt-1.5 text-xs text-muted-foreground">
          点击上方"新建 Skill"添加手动创建、远端导入或工作区内嵌 Skill
        </p>
      </div>
    );
  }

  return (
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
      {skills.map((skill) => (
        <article
          key={skill.id}
          className="flex flex-col rounded-[8px] border border-border bg-background p-3.5 transition-colors hover:border-primary/25 hover:bg-secondary/30"
        >
          {/* Card header: name + origin badge */}
          <header className="flex items-start justify-between gap-2">
            <div className="min-w-0">
              <p className="truncate text-sm font-medium leading-6 text-foreground">
                {skill.display_name}
              </p>
              <p className="mt-0.5 truncate text-xs text-muted-foreground">
                skills/{skill.key}/SKILL.md
              </p>
            </div>
            <OriginBadge skill={skill} />
          </header>

          {/* Description */}
          <p className="mt-1.5 line-clamp-2 text-xs leading-5 text-muted-foreground">
            {skill.description}
          </p>

          {/* Meta tags */}
          <div className="mt-3 flex flex-wrap gap-1.5 text-[11px] text-muted-foreground">
            <span className="rounded-[6px] border border-border bg-secondary/40 px-1.5 py-0.5">
              {skill.files.length} file{skill.files.length !== 1 ? "s" : ""}
            </span>
            {skill.disable_model_invocation && (
              <span className="rounded-[6px] border border-amber-500/30 bg-amber-500/10 px-1.5 py-0.5 text-amber-700 dark:text-amber-300">
                explicit only
              </span>
            )}
            {skill.remote_source?.digest && (
              <span
                title={`digest: ${skill.remote_source.digest}`}
                className="rounded-[6px] border border-border bg-secondary/30 px-1.5 py-0.5 text-muted-foreground/70"
              >
                imported
              </span>
            )}
          </div>

          {/* Card footer: actions */}
          <footer className="mt-3 flex items-center justify-end gap-1 border-t border-border/70 pt-2.5">
            {skill.source === "builtin_seed" && (
              <button
                type="button"
                onClick={() => onReset(skill)}
                disabled={busyId === skill.id}
                className="rounded-[6px] px-1.5 py-0.5 text-[11px] text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:opacity-50"
              >
                Reset
              </button>
            )}
            <button
              type="button"
              onClick={() => onEdit(skill)}
              className="rounded-[6px] px-1.5 py-0.5 text-[11px] text-foreground/80 transition-colors hover:bg-secondary hover:text-foreground"
            >
              编辑
            </button>
            <button
              type="button"
              onClick={() => onDelete(skill)}
              disabled={busyId === skill.id}
              className="rounded-[6px] px-1.5 py-0.5 text-[11px] text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-50"
            >
              {busyId === skill.id ? "处理中..." : "删除"}
            </button>
          </footer>
        </article>
      ))}
    </div>
  );
}

// ─── Skill Editor Dialog ─────────────────────────────────
//
// 复用原有编辑 / 创建逻辑，保持 VFS 浏览器模式。

function SkillEditorDialog({
  mode,
  projectId,
  draft,
  isSaving,
  onDraftChange,
  onClose,
  onSave,
}: {
  mode: "create" | "edit";
  projectId: string;
  draft: SkillAssetDraft;
  isSaving: boolean;
  onDraftChange: (draft: SkillAssetDraft) => void;
  onClose: () => void;
  onSave: () => void;
}) {
  const updateField = <K extends keyof SkillAssetDraft>(key: K, value: SkillAssetDraft[K]) => {
    onDraftChange({ ...draft, [key]: value });
  };
  const skillRootPath = draft.key ? `skills/${draft.key}` : "";

  if (mode === "edit" && skillRootPath) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6" onClick={onClose}>
        <div
          className="flex h-[88vh] w-[1120px] max-w-full flex-col overflow-hidden rounded-[8px] border border-border bg-background shadow-xl"
          onClick={(e) => e.stopPropagation()}
        >
          <header className="flex items-center justify-between border-b border-border px-5 py-4">
            <div>
              <h3 className="text-sm font-semibold text-foreground">编辑 Skill</h3>
              <p className="mt-0.5 text-xs text-muted-foreground">{skillRootPath}/SKILL.md</p>
            </div>
            <button type="button" onClick={onClose} className="agentdash-button-secondary">
              关闭
            </button>
          </header>
          <div className="min-h-0 flex-1">
            <VfsBrowser
              source={{ source_type: "project_skill_assets", project_id: projectId }}
              visibleMountIds={["skill-assets"]}
              initialMountId="skill-assets"
              initialFilePath={`${skillRootPath}/SKILL.md`}
              rootPath={skillRootPath}
              protectedFilePaths={[`${skillRootPath}/SKILL.md`]}
              browserHeightClassName="min-h-0 flex-1"
              className="flex h-full flex-col"
              renderInspector={(ctx) => <SkillVfsInspector context={ctx} />}
            />
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6" onClick={onClose}>
      <div
        className="flex max-h-[88vh] w-[920px] max-w-full flex-col overflow-hidden rounded-[8px] border border-border bg-background shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="flex items-center justify-between border-b border-border px-5 py-4">
          <div>
            <h3 className="text-sm font-semibold text-foreground">
              {mode === "create" ? "新建 Skill" : "编辑 Skill"}
            </h3>
            <p className="mt-0.5 text-xs text-muted-foreground">
              {draft.key ? `skills/${draft.key}/SKILL.md` : "skills/<key>/SKILL.md"}
            </p>
          </div>
          <button type="button" onClick={onClose} className="agentdash-button-secondary">
            关闭
          </button>
        </header>

        <div className="grid min-h-0 flex-1 grid-cols-1 gap-4 overflow-y-auto p-5 lg:grid-cols-[320px_minmax(0,1fr)]">
          <section className="space-y-4">
            <label className="block space-y-1.5">
              <span className="agentdash-form-label">显示名称</span>
              <input
                value={draft.display_name}
                onChange={(e) => updateField("display_name", e.target.value)}
                className="agentdash-form-input"
                placeholder="My Skill"
              />
            </label>
            <SkillYamlMetaPanel draft={draft} onChange={onDraftChange} />
            <SkillExtraFilesEditor
              files={draft.files}
              onChange={(files) => updateField("files", files)}
            />
          </section>
          <section className="flex min-h-[420px] flex-col space-y-1.5">
            <span className="agentdash-form-label">SKILL.md 正文</span>
            <textarea
              value={draft.body}
              onChange={(e) => updateField("body", e.target.value)}
              className="min-h-[420px] flex-1 resize-y rounded-[8px] border border-border bg-background px-3 py-2 font-mono text-sm leading-6 outline-none transition-colors focus:border-primary"
              placeholder="# 使用说明"
            />
          </section>
        </div>

        <footer className="flex justify-end gap-2 border-t border-border px-5 py-4">
          <button type="button" onClick={onClose} className="agentdash-button-secondary">
            取消
          </button>
          <button type="button" onClick={onSave} disabled={isSaving} className="agentdash-button-primary">
            {isSaving ? "保存中..." : "保存"}
          </button>
        </footer>
      </div>
    </div>
  );
}

// ─── VFS Inspector ───────────────────────────────────────

function SkillVfsInspector({ context }: { context: VfsBrowserPanelInspectorContext }) {
  const isSkillDocument = context.displayPath === "SKILL.md";
  const parsed = useMemo(
    () => (context.fileContent && isSkillDocument ? parseSkillMarkdown(context.fileContent) : null),
    [context.fileContent, isSkillDocument],
  );
  const [description, setDescription] = useState("");
  const [disableModelInvocation, setDisableModelInvocation] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  useEffect(() => {
    if (!parsed) return;
    setDescription(parsed.description ?? "");
    setDisableModelInvocation(parsed.disable_model_invocation);
    setSaveError(null);
  }, [parsed]);

  const dirty = Boolean(
    parsed &&
      (description !== (parsed.description ?? "") ||
        disableModelInvocation !== parsed.disable_model_invocation),
  );

  const saveMeta = useCallback(async () => {
    if (!context.fileContent || context.readOnly || !parsed) return;
    setSaving(true);
    setSaveError(null);
    try {
      const nextContent = updateSkillMarkdownFrontmatter(context.fileContent, {
        description,
        disable_model_invocation: disableModelInvocation,
      });
      await context.saveFile(nextContent);
    } catch (err) {
      setSaveError(err instanceof Error ? err.message : "保存 YAML meta 失败");
    } finally {
      setSaving(false);
    }
  }, [context, description, disableModelInvocation, parsed]);

  if (!context.filePath) {
    return (
      <aside className="flex h-full flex-col justify-center px-4 text-center text-xs text-muted-foreground">
        未选择文件
      </aside>
    );
  }

  if (!isSkillDocument || !parsed) {
    return (
      <aside className="space-y-4 p-4">
        <InspectorHeader title="文件" badge={context.mount?.displayName ?? context.mountId ?? "mount"} />
        <dl className="space-y-3 text-xs">
          <InspectorRow label="path" value={context.displayPath ?? context.filePath} mono />
          <InspectorRow label="mount" value={context.mountId ?? "-"} mono />
          <InspectorRow label="provider" value={context.mount?.provider ?? "-"} />
          <InspectorRow label="mode" value={context.readOnly ? "readonly" : "editable"} />
          <InspectorRow label="size" value={formatBytes(context.fileContent?.length ?? 0)} />
        </dl>
      </aside>
    );
  }

  return (
    <aside className="space-y-4 p-4">
      <InspectorHeader title="YAML meta" badge="SKILL.md" />

      <section className="space-y-3 rounded-[8px] border border-border bg-background p-3">
        <label className="block space-y-1.5">
          <span className="agentdash-form-label">name</span>
          <input value={parsed.name ?? ""} readOnly className="agentdash-form-input font-mono text-[12px] opacity-80" />
        </label>
        <label className="block space-y-1.5">
          <span className="agentdash-form-label">description</span>
          <textarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            readOnly={context.readOnly}
            className="agentdash-form-textarea min-h-24"
            rows={4}
          />
        </label>
        <label className="flex items-center gap-2 rounded-[7px] border border-border bg-secondary/20 px-3 py-2">
          <input
            type="checkbox"
            checked={disableModelInvocation}
            disabled={context.readOnly}
            onChange={(e) => setDisableModelInvocation(e.target.checked)}
          />
          <span className="text-xs text-foreground">disable-model-invocation</span>
        </label>
        {saveError && (
          <p className="rounded-[6px] border border-destructive/20 bg-destructive/5 px-2 py-1.5 text-xs text-destructive">
            {saveError}
          </p>
        )}
        <div className="flex items-center justify-between gap-2 border-t border-border/70 pt-3">
          <span className="text-[10px] text-muted-foreground">{dirty ? "已修改" : "已同步"}</span>
          <button
            type="button"
            onClick={() => void saveMeta()}
            disabled={context.readOnly || saving || !dirty}
            className="rounded-[6px] border border-emerald-500/30 bg-emerald-500/10 px-2 py-1 text-[11px] text-emerald-600 transition-colors hover:bg-emerald-500/20 disabled:opacity-50"
          >
            {saving ? "保存中..." : "保存 meta"}
          </button>
        </div>
      </section>

      <section className="space-y-2 rounded-[8px] border border-border bg-background p-3">
        <div className="flex items-center justify-between">
          <p className="agentdash-form-label">Frontmatter</p>
          <span className="text-[10px] text-muted-foreground">{formatBytes(parsed.frontmatter?.length ?? 0)}</span>
        </div>
        <pre className="max-h-48 overflow-auto rounded-[7px] border border-border bg-secondary/20 px-3 py-2 font-mono text-[11px] leading-5 text-muted-foreground">
          {parsed.frontmatter ?? ""}
        </pre>
      </section>

      <section className="space-y-2 rounded-[8px] border border-border bg-background p-3">
        <p className="agentdash-form-label">File</p>
        <dl className="space-y-2 text-xs">
          <InspectorRow label="path" value={context.displayPath ?? context.filePath} mono />
          <InspectorRow label="mode" value={context.readOnly ? "readonly" : "editable"} />
          <InspectorRow label="size" value={formatBytes(context.fileContent?.length ?? 0)} />
        </dl>
      </section>
    </aside>
  );
}

// ─── Inspector Helpers ───────────────────────────────────

function InspectorHeader({ title, badge }: { title: string; badge: string }) {
  return (
    <header className="flex items-center justify-between gap-3">
      <h4 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{title}</h4>
      <span className="max-w-[160px] truncate rounded-[6px] border border-border bg-background px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
        {badge}
      </span>
    </header>
  );
}

function InspectorRow({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="space-y-1">
      <dt className="agentdash-form-label">{label}</dt>
      <dd className={`break-words text-foreground/85 ${mono ? "font-mono text-[11px]" : ""}`}>{value}</dd>
    </div>
  );
}

function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

// ─── YAML Meta Panel ─────────────────────────────────────

function SkillYamlMetaPanel({
  draft,
  onChange,
}: {
  draft: SkillAssetDraft;
  onChange: (draft: SkillAssetDraft) => void;
}) {
  const patchDraft = <K extends keyof SkillAssetDraft>(key: K, value: SkillAssetDraft[K]) => {
    onChange({ ...draft, [key]: value });
  };

  return (
    <section className="space-y-3 rounded-[8px] border border-border bg-secondary/20 p-3">
      <div className="flex items-center justify-between gap-3">
        <p className="agentdash-form-label">YAML meta</p>
        <span className="rounded-[6px] border border-border bg-background px-1.5 py-0.5 text-[10px] text-muted-foreground">
          SKILL.md
        </span>
      </div>
      <label className="block space-y-1.5">
        <span className="agentdash-form-label">name</span>
        <input
          value={draft.key}
          onChange={(e) => patchDraft("key", e.target.value)}
          className="agentdash-form-input"
          placeholder="my-skill"
        />
      </label>
      <label className="block space-y-1.5">
        <span className="agentdash-form-label">description</span>
        <textarea
          value={draft.description}
          onChange={(e) => patchDraft("description", e.target.value)}
          className="agentdash-form-textarea"
          rows={3}
        />
      </label>
      <label className="flex items-center gap-2 rounded-[7px] border border-border bg-background px-3 py-2">
        <input
          type="checkbox"
          checked={draft.disable_model_invocation}
          onChange={(e) => patchDraft("disable_model_invocation", e.target.checked)}
        />
        <span className="text-xs text-foreground">disable-model-invocation</span>
      </label>
      <pre className="max-h-40 overflow-auto rounded-[7px] border border-border bg-background px-3 py-2 font-mono text-[11px] leading-5 text-muted-foreground">
        {buildSkillYamlFrontmatter(draft)}
      </pre>
    </section>
  );
}

// ─── Extra Files Editor ──────────────────────────────────

function SkillExtraFilesEditor({
  files,
  onChange,
}: {
  files: SkillAssetDraft["files"];
  onChange: (files: SkillAssetDraft["files"]) => void;
}) {
  const [selectedPath, setSelectedPath] = useState<string | null>(files[0]?.relative_path ?? null);
  const selectedFile = files.find((f) => f.relative_path === selectedPath) ?? files[0] ?? null;

  const createFile = () => {
    const path = window.prompt("新建附加文件路径", nextExtraFilePath(files));
    const normalizedPath = normalizeSkillExtraPath(path ?? "");
    if (!normalizedPath || files.some((f) => f.relative_path === normalizedPath)) return;
    onChange([...files, { relative_path: normalizedPath, content: "" }]);
    setSelectedPath(normalizedPath);
  };

  const renameFile = () => {
    if (!selectedFile) return;
    const path = window.prompt("重命名附加文件", selectedFile.relative_path);
    const normalizedPath = normalizeSkillExtraPath(path ?? "");
    if (!normalizedPath || normalizedPath === selectedFile.relative_path) return;
    if (files.some((f) => f.relative_path === normalizedPath)) return;
    onChange(
      files.map((f) =>
        f.relative_path === selectedFile.relative_path
          ? { ...f, relative_path: normalizedPath }
          : f,
      ),
    );
    setSelectedPath(normalizedPath);
  };

  const deleteFile = () => {
    if (!selectedFile) return;
    if (!window.confirm(`删除附加文件「${selectedFile.relative_path}」？`)) return;
    const nextFiles = files.filter((f) => f.relative_path !== selectedFile.relative_path);
    onChange(nextFiles);
    setSelectedPath(nextFiles[0]?.relative_path ?? null);
  };

  const saveContent = (content: string) => {
    if (!selectedFile) return;
    onChange(
      files.map((f) =>
        f.relative_path === selectedFile.relative_path ? { ...f, content } : f,
      ),
    );
  };

  return (
    <section className="overflow-hidden rounded-[8px] border border-border">
      <header className="flex items-center justify-between border-b border-border bg-secondary/20 px-3 py-2">
        <p className="agentdash-form-label">附加文件</p>
        <div className="flex items-center gap-1">
          <SkillFileActionButton title="新建附加文件" onClick={createFile}>
            <PlusIcon />
          </SkillFileActionButton>
          <SkillFileActionButton title="重命名附加文件" onClick={renameFile} disabled={!selectedFile}>
            <RenameIcon />
          </SkillFileActionButton>
          <SkillFileActionButton title="删除附加文件" onClick={deleteFile} disabled={!selectedFile} danger>
            <TrashIcon />
          </SkillFileActionButton>
        </div>
      </header>
      <div className="grid min-h-[360px] grid-cols-[180px_minmax(0,1fr)]">
        <div className="border-r border-border bg-secondary/10">
          {files.length === 0 ? (
            <div className="px-3 py-4 text-center text-xs text-muted-foreground">无附加文件</div>
          ) : (
            <div className="max-h-[360px] overflow-auto py-1">
              {files.map((file) => {
                const selected = file.relative_path === selectedFile?.relative_path;
                return (
                  <button
                    key={file.relative_path}
                    type="button"
                    onClick={() => setSelectedPath(file.relative_path)}
                    className={`flex w-full items-center gap-1.5 px-2 py-1.5 text-left font-mono text-[11px] transition-colors hover:bg-secondary/60 ${
                      selected ? "bg-primary/8 text-foreground" : "text-muted-foreground"
                    }`}
                  >
                    <span className="shrink-0 text-muted-foreground/60">#</span>
                    <span className="min-w-0 flex-1 truncate">{file.relative_path}</span>
                  </button>
                );
              })}
            </div>
          )}
        </div>
        <div className="min-w-0">
          {selectedFile ? (
            <VfsCodeEditor
              key={selectedFile.relative_path}
              content={selectedFile.content}
              filePath={selectedFile.relative_path}
              onSave={saveContent}
            />
          ) : (
            <div className="flex h-full items-center justify-center px-4 text-center text-xs text-muted-foreground">
              选择或新建附加文件
            </div>
          )}
        </div>
      </div>
    </section>
  );
}

function nextExtraFilePath(files: SkillAssetDraft["files"]): string {
  let index = 1;
  let path = "references/notes.md";
  const used = new Set(files.map((f) => f.relative_path));
  while (used.has(path)) {
    index += 1;
    path = `references/notes-${index}.md`;
  }
  return path;
}

// ─── Shared UI Atoms ─────────────────────────────────────

function SkillFileActionButton({
  children,
  title,
  disabled,
  danger = false,
  onClick,
}: {
  children: ReactNode;
  title: string;
  disabled?: boolean;
  danger?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      title={title}
      aria-label={title}
      onClick={onClick}
      disabled={disabled}
      className={`inline-flex h-7 w-7 items-center justify-center rounded-[4px] border transition-colors disabled:cursor-not-allowed disabled:opacity-40 ${
        danger
          ? "border-destructive/25 text-destructive hover:bg-destructive/10"
          : "border-border bg-background text-muted-foreground hover:bg-secondary hover:text-foreground"
      }`}
    >
      {children}
    </button>
  );
}

function PlusIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M12 5v14" />
      <path d="M5 12h14" />
    </svg>
  );
}

function RenameIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M12 20h9" />
      <path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z" />
    </svg>
  );
}

function TrashIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M3 6h18" />
      <path d="M8 6V4h8v2" />
      <path d="M19 6l-1 14H6L5 6" />
      <path d="M10 11v6" />
      <path d="M14 11v6" />
    </svg>
  );
}

// ─── Confirm Delete Dialog ───────────────────────────────

function ConfirmDeleteDialog({
  skill,
  busy,
  onCancel,
  onConfirm,
}: {
  skill: SkillAssetDto;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onClick={onCancel}>
      <div
        className="w-[380px] rounded-[8px] border border-border bg-background p-5 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="text-sm font-semibold text-foreground">确认删除</h3>
        <p className="mt-2 text-xs leading-5 text-muted-foreground">
          确定要删除 Skill <span className="font-medium text-foreground">{skill.key}</span> 吗？
        </p>
        <div className="mt-4 flex justify-end gap-2">
          <button type="button" onClick={onCancel} className="agentdash-button-secondary">
            取消
          </button>
          <button type="button" onClick={onConfirm} disabled={busy} className="agentdash-button-danger">
            {busy ? "删除中..." : "删除"}
          </button>
        </div>
      </div>
    </div>
  );
}

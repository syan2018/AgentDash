import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { useProjectStore } from "../../../stores/projectStore";
import {
  bootstrapSkillAssets,
  buildSkillYamlFrontmatter,
  createEmptySkillAssetDraft,
  createSkillAsset,
  deleteSkillAsset,
  draftFromSkillAsset,
  dtoFilesFromDraft,
  fetchProjectSkillAssets,
  normalizeSkillExtraPath,
  resetSkillAssetFromBuiltin,
  updateSkillAsset,
  uploadSkillAssets,
  validateSkillAssetDraft,
  type SkillAssetDraft,
} from "../../../services/skillAsset";
import type { SkillAssetDto } from "../../../types";

type DetailMode =
  | { kind: "closed" }
  | { kind: "create" }
  | { kind: "edit"; assetId: string; originalKey: string };

function cloneDraft(draft: SkillAssetDraft): SkillAssetDraft {
  return {
    ...draft,
    files: draft.files.map((file) => ({ ...file })),
  };
}

export function SkillCategoryPanel() {
  const currentProjectId = useProjectStore((s) => s.currentProjectId);
  const projects = useProjectStore((s) => s.projects);
  const currentProject = useMemo(
    () => projects.find((project) => project.id === currentProjectId) ?? null,
    [currentProjectId, projects],
  );

  const zipInputRef = useRef<HTMLInputElement | null>(null);
  const directoryInputRef = useRef<HTMLInputElement | null>(null);
  const [skills, setSkills] = useState<SkillAssetDto[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [detail, setDetail] = useState<DetailMode>({ kind: "closed" });
  const [draft, setDraft] = useState<SkillAssetDraft>(() => createEmptySkillAssetDraft());
  const [confirmDelete, setConfirmDelete] = useState<SkillAssetDto | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

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

  const openCreate = useCallback(() => {
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
        .filter((file) => normalizeSkillExtraPath(file.relative_path))
        .map((file) => ({
          relative_path: normalizeSkillExtraPath(file.relative_path),
          content: file.content,
        })),
    };
    const existingKeys =
      detail.kind === "edit"
        ? skills.map((skill) => skill.key).filter((key) => key !== detail.originalKey)
        : skills.map((skill) => skill.key);
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

  const handleUpload = useCallback(
    async (fileList: FileList | null) => {
      if (!currentProjectId || !fileList || fileList.length === 0) return;
      setIsSaving(true);
      setError(null);
      try {
        const uploaded = await uploadSkillAssets(currentProjectId, Array.from(fileList));
        setMessage(`已导入 ${uploaded.length} 个 Skill`);
        await loadSkills();
      } catch (e) {
        setError(e instanceof Error ? e.message : "上传 Skill 失败");
      } finally {
        setIsSaving(false);
        if (zipInputRef.current) zipInputRef.current.value = "";
        if (directoryInputRef.current) directoryInputRef.current.value = "";
      }
    },
    [currentProjectId, loadSkills],
  );

  const handleBootstrap = useCallback(async () => {
    if (!currentProjectId) return;
    setIsSaving(true);
    setError(null);
    try {
      const bootstrapped = await bootstrapSkillAssets(currentProjectId);
      setMessage(`已装载 ${bootstrapped.length} 个内嵌 Skill`);
      await loadSkills();
    } catch (e) {
      setError(e instanceof Error ? e.message : "装载内嵌 Skill 失败");
    } finally {
      setIsSaving(false);
    }
  }, [currentProjectId, loadSkills]);

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

  if (!currentProjectId || !currentProject) {
    return (
      <div className="flex h-full items-center justify-center p-6">
        <div className="text-center text-sm text-muted-foreground">请选择项目后查看 Skill 资产</div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div className="space-y-1">
          <h2 className="text-base font-semibold tracking-tight text-foreground">Skill 资产</h2>
          <p className="text-xs text-muted-foreground">
            {skills.length} 个 project Skill · Agent preset 可按 key 装载
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <input
            ref={zipInputRef}
            type="file"
            accept=".zip"
            className="hidden"
            onChange={(event) => void handleUpload(event.currentTarget.files)}
          />
          <input
            ref={directoryInputRef}
            type="file"
            multiple
            className="hidden"
            onChange={(event) => void handleUpload(event.currentTarget.files)}
            {...{ webkitdirectory: "true", directory: "true" }}
          />
          <button type="button" onClick={handleBootstrap} disabled={isSaving} className="agentdash-button-secondary">
            Bootstrap
          </button>
          <button type="button" onClick={() => directoryInputRef.current?.click()} disabled={isSaving} className="agentdash-button-secondary">
            上传目录
          </button>
          <button type="button" onClick={() => zipInputRef.current?.click()} disabled={isSaving} className="agentdash-button-secondary">
            上传 ZIP
          </button>
          <button type="button" onClick={openCreate} disabled={isSaving} className="agentdash-button-primary">
            新建 Skill
          </button>
        </div>
      </header>

      {message && (
        <Notice tone="success" message={message} onClose={() => setMessage(null)} />
      )}
      {error && (
        <Notice tone="danger" message={error} onClose={() => setError(null)} />
      )}

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

      {detail.kind !== "closed" && (
        <SkillEditorDialog
          mode={detail.kind}
          draft={draft}
          isSaving={isSaving}
          onDraftChange={setDraft}
          onClose={() => setDetail({ kind: "closed" })}
          onSave={() => void handleSaveDraft()}
        />
      )}

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
      <div className="rounded-[8px] border border-dashed border-border bg-secondary/20 px-6 py-10 text-center">
        <p className="text-sm text-foreground">暂无 Skill 资产</p>
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
          <header className="flex items-start justify-between gap-2">
            <div className="min-w-0">
              <p className="truncate text-sm font-medium leading-6 text-foreground">{skill.display_name}</p>
              <p className="mt-0.5 truncate text-xs text-muted-foreground">
                skills/{skill.key}/SKILL.md
              </p>
            </div>
            <span className="shrink-0 rounded-[6px] border border-border bg-secondary/50 px-1.5 py-0.5 text-[10px] text-muted-foreground">
              {skill.source === "builtin_seed" ? "builtin" : "user"}
            </span>
          </header>

          <p className="mt-1.5 line-clamp-2 text-xs leading-5 text-muted-foreground">
            {skill.description}
          </p>

          <div className="mt-3 flex flex-wrap gap-1.5 text-[11px] text-muted-foreground">
            <span className="rounded-[6px] border border-border bg-secondary/40 px-1.5 py-0.5">
              {skill.files.length} file
            </span>
            {skill.disable_model_invocation && (
              <span className="rounded-[6px] border border-amber-500/30 bg-amber-500/10 px-1.5 py-0.5 text-amber-700 dark:text-amber-300">
                explicit
              </span>
            )}
          </div>

          <footer className="mt-3 flex items-center justify-end gap-1 border-t border-border/70 pt-2.5 text-[11px] text-muted-foreground">
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

function SkillEditorDialog({
  mode,
  draft,
  isSaving,
  onDraftChange,
  onClose,
  onSave,
}: {
  mode: "create" | "edit";
  draft: SkillAssetDraft;
  isSaving: boolean;
  onDraftChange: (draft: SkillAssetDraft) => void;
  onClose: () => void;
  onSave: () => void;
}) {
  const updateField = <K extends keyof SkillAssetDraft>(key: K, value: SkillAssetDraft[K]) => {
    onDraftChange({ ...draft, [key]: value });
  };

  const updateFile = (index: number, patch: Partial<SkillAssetDraft["files"][number]>) => {
    onDraftChange({
      ...draft,
      files: draft.files.map((file, i) => (i === index ? { ...file, ...patch } : file)),
    });
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6" onClick={onClose}>
      <div
        className="flex max-h-[88vh] w-[920px] max-w-full flex-col overflow-hidden rounded-[8px] border border-border bg-background shadow-xl"
        onClick={(event) => event.stopPropagation()}
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
              <input value={draft.display_name} onChange={(event) => updateField("display_name", event.target.value)} className="agentdash-form-input" placeholder="My Skill" />
            </label>

            <SkillYamlMetaPanel draft={draft} onChange={onDraftChange} />

            <section className="space-y-2">
              <div className="flex items-center justify-between">
                <p className="agentdash-form-label">附加文件</p>
                <button
                  type="button"
                  onClick={() => updateField("files", [...draft.files, { relative_path: "references/notes.md", content: "" }])}
                  className="rounded-[7px] border border-border px-2 py-1 text-[11px] text-foreground transition-colors hover:bg-secondary"
                >
                  + 文件
                </button>
              </div>
              {draft.files.length === 0 ? (
                <div className="rounded-[8px] border border-dashed border-border bg-secondary/20 px-3 py-4 text-center text-xs text-muted-foreground">
                  无附加文件
                </div>
              ) : (
                <div className="space-y-2">
                  {draft.files.map((file, index) => (
                    <div key={`${index}:${file.relative_path}`} className="rounded-[8px] border border-border p-2">
                      <div className="flex gap-2">
                        <input
                          value={file.relative_path}
                          onChange={(event) => updateFile(index, { relative_path: event.target.value })}
                          className="h-8 min-w-0 flex-1 rounded-[7px] border border-border bg-background px-2 text-xs outline-none transition-colors focus:border-primary"
                          placeholder="references/api.md"
                        />
                        <button
                          type="button"
                          onClick={() => updateField("files", draft.files.filter((_, i) => i !== index))}
                          className="rounded-[7px] px-2 text-xs text-destructive transition-colors hover:bg-destructive/10"
                        >
                          删除
                        </button>
                      </div>
                      <textarea
                        value={file.content}
                        onChange={(event) => updateFile(index, { content: event.target.value })}
                        className="mt-2 min-h-24 w-full resize-y rounded-[7px] border border-border bg-background px-2 py-1.5 font-mono text-xs outline-none transition-colors focus:border-primary"
                      />
                    </div>
                  ))}
                </div>
              )}
            </section>
          </section>

          <section className="flex min-h-[420px] flex-col space-y-1.5">
            <span className="agentdash-form-label">SKILL.md 正文</span>
            <textarea
              value={draft.body}
              onChange={(event) => updateField("body", event.target.value)}
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
          onChange={(event) => patchDraft("key", event.target.value)}
          className="agentdash-form-input"
          placeholder="my-skill"
        />
      </label>

      <label className="block space-y-1.5">
        <span className="agentdash-form-label">description</span>
        <textarea
          value={draft.description}
          onChange={(event) => patchDraft("description", event.target.value)}
          className="agentdash-form-textarea"
          rows={3}
        />
      </label>

      <label className="flex items-center gap-2 rounded-[7px] border border-border bg-background px-3 py-2">
        <input
          type="checkbox"
          checked={draft.disable_model_invocation}
          onChange={(event) => patchDraft("disable_model_invocation", event.target.checked)}
        />
        <span className="text-xs text-foreground">disable-model-invocation</span>
      </label>

      <pre className="max-h-40 overflow-auto rounded-[7px] border border-border bg-background px-3 py-2 font-mono text-[11px] leading-5 text-muted-foreground">
        {buildSkillYamlFrontmatter(draft)}
      </pre>
    </section>
  );
}

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
      <div className="w-[380px] rounded-[8px] border border-border bg-background p-5 shadow-xl" onClick={(event) => event.stopPropagation()}>
        <h3 className="text-sm font-semibold text-foreground">确认删除</h3>
        <p className="mt-2 text-xs leading-5 text-muted-foreground">
          确定要删除 Skill <span className="font-medium text-foreground">{skill.key}</span> 吗？
        </p>
        <div className="mt-4 flex justify-end gap-2">
          <button type="button" onClick={onCancel} className="agentdash-button-secondary">取消</button>
          <button type="button" onClick={onConfirm} disabled={busy} className="agentdash-button-danger">
            {busy ? "删除中..." : "删除"}
          </button>
        </div>
      </div>
    </div>
  );
}

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
import { useCurrentUserStore } from "../../../stores/currentUserStore";
import { formatBytes } from "../../../lib/format";
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
  updateSkillMarkdownFrontmatter,
  updateSkillAsset,
  validateSkillAssetDraft,
  type SkillAssetDraft,
} from "../../../services/skillAsset";
import type { LibraryAssetDto, SkillAssetDto } from "../../../types";
import { CreateSkillDialog } from "./CreateSkillDialog";
import {
  AssetCard,
  CardMenu,
  CreateButton,
  DangerConfirmDialog,
  type DismissibleNoticeData,
  InspectorRow as UiInspectorRow,
  MetaTagList,
  type MetaTagItem,
  OriginBadge as UiOriginBadge,
  PromptDialog,
  SectionTitle as UiSectionTitle,
} from "@agentdash/ui";
import { buildAssetMenuItems } from "../_shared/assetMenu";
import { CategoryPageShell } from "../_shared/CategoryPageShell";
import { PublishedBadge } from "../_shared/PublishedBadge";
import { SelectProjectEmpty } from "../_shared/SelectProjectEmpty";
import { useLibraryPublishedAssets } from "../_shared/useLibraryPublishedAssets";
import { PublishLibraryAssetDialog } from "../publish/PublishLibraryAssetDialog";
import { resolveOriginBadge } from "../_shared/origin-badge-tone";

// ─── Detail mode ─────────────────────────────────────────

type DetailMode =
  | { kind: "closed" }
  | { kind: "create" }
  | { kind: "edit"; assetId: string; originalKey: string };

function cloneDraft(draft: SkillAssetDraft): SkillAssetDraft {
  return {
    ...draft,
    files: draft.files.map((f) => ({ ...f })),
    binary_files: draft.binary_files.map((f) => ({ ...f })),
  };
}

// ─── Main Panel ──────────────────────────────────────────

export function SkillCategoryPanel() {
  const currentProjectId = useProjectStore((s) => s.currentProjectId);
  const projects = useProjectStore((s) => s.projects);
  const currentProject = useMemo(
    () => projects.find((p) => p.id === currentProjectId) ?? null,
    [currentProjectId, projects],
  );

  const currentUserId = useCurrentUserStore((s) => s.currentUser?.user_id ?? null);

  const [skills, setSkills] = useState<SkillAssetDto[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [detail, setDetail] = useState<DetailMode>({ kind: "closed" });
  const [draft, setDraft] = useState<SkillAssetDraft>(() => createEmptySkillAssetDraft());
  const [confirmDelete, setConfirmDelete] = useState<SkillAssetDto | null>(null);
  const [publishTarget, setPublishTarget] = useState<SkillAssetDto | null>(null);
  const { publishedByKey, reloadPublished } = useLibraryPublishedAssets("skill_template");
  const [showCreateDialog, setShowCreateDialog] = useState(false);
  const [notice, setNotice] = useState<DismissibleNoticeData | null>(null);
  const showSuccess = useCallback((msg: string) => setNotice({ tone: "success", message: msg }), []);
  const showError = useCallback((msg: string) => setNotice({ tone: "danger", message: msg }), []);
  const clearNotice = useCallback(() => setNotice(null), []);

  // ── Data loading ────────────────────────────────────

  const loadSkills = useCallback(async () => {
    if (!currentProjectId) return;
    setIsLoading(true);
    clearNotice();
    try {
      setSkills(await fetchProjectSkillAssets(currentProjectId));
    } catch (e) {
      showError(e instanceof Error ? e.message : "加载 Skill 资产失败");
    } finally {
      setIsLoading(false);
    }
  }, [currentProjectId, clearNotice, showError]);

  useEffect(() => {
    void loadSkills();
  }, [loadSkills]);


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
    clearNotice();
    setDetail({ kind: "create" });
  }, [clearNotice]);

  const openEdit = useCallback((skill: SkillAssetDto) => {
    setDraft(cloneDraft(draftFromSkillAsset(skill)));
    clearNotice();
    setDetail({ kind: "edit", assetId: skill.id, originalKey: skill.key });
  }, [clearNotice]);

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
      binary_files: draft.binary_files,
    };
    const existingKeys =
      detail.kind === "edit"
        ? skills.map((s) => s.key).filter((k) => k !== detail.originalKey)
        : skills.map((s) => s.key);
    const validation = validateSkillAssetDraft(normalizedDraft, existingKeys);
    if (!validation.ok) {
      showError(validation.message ?? "Skill 表单校验失败");
      return;
    }

    setIsSaving(true);
    clearNotice();
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
      showSuccess(`已保存 Skill：${normalizedDraft.key}`);
      setDetail({ kind: "closed" });
      await loadSkills();
    } catch (e) {
      showError(e instanceof Error ? e.message : "保存 Skill 资产失败");
    } finally {
      setIsSaving(false);
    }
  }, [currentProjectId, detail, draft, loadSkills, skills, clearNotice, showSuccess, showError]);

  const handleDelete = useCallback(async () => {
    if (!currentProjectId || !confirmDelete) return;
    setBusyId(confirmDelete.id);
    clearNotice();
    try {
      await deleteSkillAsset(currentProjectId, confirmDelete.id);
      showSuccess(`已删除 Skill：${confirmDelete.key}`);
      if (detail.kind === "edit" && detail.assetId === confirmDelete.id) {
        setDetail({ kind: "closed" });
      }
      setConfirmDelete(null);
      await loadSkills();
    } catch (e) {
      showError(e instanceof Error ? e.message : "删除 Skill 资产失败");
    } finally {
      setBusyId(null);
    }
  }, [confirmDelete, currentProjectId, detail, loadSkills, clearNotice, showSuccess, showError]);

  const handleCreateDialogCreated = useCallback(
    (msg: string) => {
      showSuccess(msg);
      setShowCreateDialog(false);
      void loadSkills();
    },
    [loadSkills, showSuccess],
  );

  // ── Guard ───────────────────────────────────────────

  if (!currentProjectId || !currentProject) {
    return <SelectProjectEmpty assetLabel="Skill 资产" />;
  }

  // ── Render ──────────────────────────────────────────

  const statsText = Object.entries(stats)
    .map(([tag, count]) => `${count} 个 ${tag}`)
    .join(" · ");

  return (
    <CategoryPageShell
      title="Skill 资产"
      stats={
        skills.length > 0
          ? `${statsText} · 可由 Agent 按 key 装载`
          : "0 个 Skill · 可由 Agent 按 key 装载"
      }
      actions={<CreateButton entity="Skill" onClick={() => setShowCreateDialog(true)} />}
      notice={notice}
      onDismissNotice={clearNotice}
    >
      {isLoading ? (
        <div className="rounded-[8px] border border-dashed border-border px-6 py-10 text-center text-sm text-muted-foreground">
          正在加载 Skill 资产…
        </div>
      ) : (
        <SkillGrid
          skills={skills}
          publishedByKey={publishedByKey}
          busyId={busyId}
          onEdit={openEdit}
          onPublish={setPublishTarget}
          onDelete={setConfirmDelete}
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

      <DangerConfirmDialog
        open={confirmDelete != null}
        title="确认删除"
        description={
          confirmDelete
            ? `确定要删除 Skill ${confirmDelete.key} 吗？此操作不可撤销。`
            : ""
        }
        confirmLabel={
          confirmDelete && busyId === confirmDelete.id ? "删除中…" : "删除"
        }
        isConfirming={confirmDelete != null && busyId === confirmDelete.id}
        onClose={() => setConfirmDelete(null)}
        onConfirm={() => void handleDelete()}
      />

      {publishTarget && (
        <PublishLibraryAssetDialog
          projectId={currentProjectId}
          assetKind="skill_asset"
          projectAssetId={publishTarget.id}
          defaults={{
            key: publishTarget.key,
            display_name: publishTarget.display_name,
            description: publishTarget.description,
          }}
          currentUserId={currentUserId}
          onClose={() => setPublishTarget(null)}
          onPublished={(message) => {
            showSuccess(message);
            void loadSkills();
            reloadPublished();
          }}
        />
      )}
    </CategoryPageShell>
  );
}

export default SkillCategoryPanel;

// ─── Origin Badge ────────────────────────────────────────

function OriginBadge({ skill }: { skill: SkillAssetDto }) {
  const { label, tone } = resolveOriginBadge(skill.source, Boolean(skill.installed_source));
  return <UiOriginBadge label={label} tone={tone} url={skill.remote_source?.url ?? null} />;
}

// ─── Skill Grid ──────────────────────────────────────────

function SkillGrid({
  skills,
  publishedByKey,
  busyId,
  onEdit,
  onPublish,
  onDelete,
}: {
  skills: SkillAssetDto[];
  publishedByKey: Map<string, LibraryAssetDto>;
  busyId: string | null;
  onEdit: (skill: SkillAssetDto) => void;
  onPublish: (skill: SkillAssetDto) => void;
  onDelete: (skill: SkillAssetDto) => void;
}) {
  if (skills.length === 0) {
    return (
      <div className="rounded-[8px] border border-dashed border-border bg-secondary/20 px-6 py-14 text-center">
        <p className="text-sm text-foreground">暂无 Skill 资产</p>
        <p className="mt-1.5 text-xs text-muted-foreground">
          点击上方"+ Skill"添加手动创建、远端导入或本地上传 Skill
        </p>
      </div>
    );
  }

  return (
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
      {skills.map((skill) => {
        const isInstalled = Boolean(skill.installed_source);
        const isBuiltin = skill.source === "builtin_seed";
        const canPublish = !isInstalled && !isBuiltin;
        const published = publishedByKey.get(skill.key) ?? null;
        const isBusy = busyId === skill.id;
        const menuItems = buildAssetMenuItems({
          primary: { label: "编辑", onSelect: () => onEdit(skill) },
          publish: canPublish
            ? { published: Boolean(published), onSelect: () => onPublish(skill) }
            : null,
          danger: { label: "删除", busy: isBusy, onSelect: () => onDelete(skill) },
        });

        const tags: MetaTagItem[] = [
          {
            key: "files",
            label: `${skill.files.length} file${skill.files.length !== 1 ? "s" : ""}`,
          },
        ];
        if (skill.disable_model_invocation) {
          tags.push({ key: "explicit", label: "explicit only", tone: "warning" });
        }
        if (skill.remote_source?.digest) {
          tags.push({
            key: "imported",
            label: "imported",
            tone: "muted",
            title: `digest: ${skill.remote_source.digest}`,
          });
        }

        return (
          <AssetCard
            key={skill.id}
            onOpen={() => onEdit(skill)}
            openTitle="编辑"
            title={skill.display_name}
            subtitle={`skills/${skill.key}/SKILL.md`}
            description={skill.description}
            headerRight={
              <>
                {published && <PublishedBadge version={published.version} />}
                <OriginBadge skill={skill} />
                <CardMenu items={menuItems} />
              </>
            }
          >
            <MetaTagList items={tags} />
          </AssetCard>
        );
      })}
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
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-3 sm:p-6" onClick={onClose}>
        <div
          className="flex h-[90vh] w-[min(94vw,1680px)] flex-col overflow-hidden rounded-[8px] border border-border bg-background shadow-xl"
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
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-3 sm:p-6" onClick={onClose}>
      <div
        className="flex max-h-[90vh] w-[min(94vw,1200px)] flex-col overflow-hidden rounded-[8px] border border-border bg-background shadow-xl"
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
      <aside className="flex h-full flex-col">
        <InspectorTitleBar
          title="文件"
          subtitle={context.mount?.displayName ?? context.mountId ?? "mount"}
        />
        <dl className="flex-1 space-y-3 overflow-y-auto px-4 py-4 text-xs">
          <InspectorRow label="path" value={context.displayPath ?? context.filePath} mono />
          <InspectorRow label="mount" value={context.mountId ?? "-"} mono />
          <InspectorRow label="provider" value={context.mount?.provider ?? "-"} />
          <InspectorRow label="mode" value={context.readOnly ? "readonly" : "editable"} />
          <InspectorRow label="size" value={formatBytes(context.fileContent?.length ?? 0)} />
        </dl>
      </aside>
    );
  }

  const statusLabel = saving ? "保存中…" : dirty ? "保存 meta" : "已同步";

  return (
    <aside className="flex h-full flex-col">
      <InspectorTitleBar title="YAML meta" subtitle="SKILL.md">
        <button
          type="button"
          onClick={() => void saveMeta()}
          disabled={context.readOnly || saving || !dirty}
          className="shrink-0 rounded-[6px] border border-success/30 bg-success/10 px-2.5 py-1 text-[11px] text-success transition-colors hover:bg-success/20 disabled:cursor-not-allowed disabled:border-border disabled:bg-transparent disabled:text-muted-foreground"
        >
          {statusLabel}
        </button>
      </InspectorTitleBar>

      <div className="flex-1 space-y-5 overflow-y-auto px-4 py-4">
        <div className="space-y-3">
          <label className="block space-y-1.5">
            <span className="agentdash-form-label">name</span>
            <input
              value={parsed.name ?? ""}
              readOnly
              className="agentdash-form-input font-mono text-[12px] opacity-80"
            />
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
          <label className="flex items-center gap-2 text-xs text-foreground">
            <input
              type="checkbox"
              checked={disableModelInvocation}
              disabled={context.readOnly}
              onChange={(e) => setDisableModelInvocation(e.target.checked)}
            />
            <span>disable-model-invocation</span>
          </label>
          {saveError && (
            <p className="rounded-[6px] border border-destructive/20 bg-destructive/5 px-2 py-1.5 text-xs text-destructive">
              {saveError}
            </p>
          )}
        </div>

        <div className="space-y-1.5">
          <div className="flex items-center justify-between">
            <p className="agentdash-form-label">frontmatter</p>
            <span className="text-[10px] text-muted-foreground">
              {formatBytes(parsed.frontmatter?.length ?? 0)}
            </span>
          </div>
          <pre className="max-h-48 overflow-auto rounded-[6px] border border-border/70 bg-background px-3 py-2 font-mono text-[11px] leading-5 text-muted-foreground">
            {parsed.frontmatter ?? ""}
          </pre>
        </div>

        <div className="space-y-2">
          <p className="agentdash-form-label">file</p>
          <dl className="space-y-2 text-xs">
            <InspectorRow label="path" value={context.displayPath ?? context.filePath} mono />
            <InspectorRow label="mode" value={context.readOnly ? "readonly" : "editable"} />
            <InspectorRow label="size" value={formatBytes(context.fileContent?.length ?? 0)} />
          </dl>
        </div>
      </div>
    </aside>
  );
}

// ─── Inspector Helpers ───────────────────────────────────

function InspectorTitleBar({
  title,
  subtitle,
  children,
}: {
  title: string;
  subtitle: string;
  children?: ReactNode;
}) {
  return <UiSectionTitle title={title} subtitle={subtitle} actions={children} sticky />;
}

function InspectorRow({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return <UiInspectorRow label={label} value={value} mono={mono} />;
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
      <label className="flex items-center gap-2 rounded-[8px] border border-border bg-background px-3 py-2">
        <input
          type="checkbox"
          checked={draft.disable_model_invocation}
          onChange={(e) => patchDraft("disable_model_invocation", e.target.checked)}
        />
        <span className="text-xs text-foreground">disable-model-invocation</span>
      </label>
      <pre className="max-h-40 overflow-auto rounded-[8px] border border-border bg-background px-3 py-2 font-mono text-[11px] leading-5 text-muted-foreground">
        {buildSkillYamlFrontmatter(draft)}
      </pre>
    </section>
  );
}

// ─── Extra Files Editor ──────────────────────────────────

type SkillExtraFilePromptState =
  | { kind: "create"; value: string }
  | { kind: "rename"; originalPath: string; value: string };

function SkillExtraFilesEditor({
  files,
  onChange,
}: {
  files: SkillAssetDraft["files"];
  onChange: (files: SkillAssetDraft["files"]) => void;
}) {
  const [selectedPath, setSelectedPath] = useState<string | null>(files[0]?.relative_path ?? null);
  const [filePrompt, setFilePrompt] = useState<SkillExtraFilePromptState | null>(null);
  const [deleteTargetPath, setDeleteTargetPath] = useState<string | null>(null);
  const selectedFile = files.find((f) => f.relative_path === selectedPath) ?? files[0] ?? null;

  const createFile = () => {
    setFilePrompt({ kind: "create", value: nextExtraFilePath(files) });
  };

  const confirmCreateFile = (path: string) => {
    const normalizedPath = normalizeSkillExtraPath(path);
    if (!normalizedPath || files.some((f) => f.relative_path === normalizedPath)) return;
    onChange([...files, { relative_path: normalizedPath, content: "" }]);
    setSelectedPath(normalizedPath);
    setFilePrompt(null);
  };

  const renameFile = () => {
    if (!selectedFile) return;
    setFilePrompt({ kind: "rename", originalPath: selectedFile.relative_path, value: selectedFile.relative_path });
  };

  const confirmRenameFile = (originalPath: string, path: string) => {
    const normalizedPath = normalizeSkillExtraPath(path);
    if (!normalizedPath || normalizedPath === originalPath) return;
    if (files.some((f) => f.relative_path === normalizedPath)) return;
    onChange(
      files.map((f) =>
        f.relative_path === originalPath
          ? { ...f, relative_path: normalizedPath }
          : f,
      ),
    );
    setSelectedPath(normalizedPath);
    setFilePrompt(null);
  };

  const deleteFile = () => {
    if (!selectedFile) return;
    setDeleteTargetPath(selectedFile.relative_path);
  };

  const confirmDeleteFile = () => {
    if (!deleteTargetPath) return;
    const nextFiles = files.filter((f) => f.relative_path !== deleteTargetPath);
    onChange(nextFiles);
    setSelectedPath(nextFiles[0]?.relative_path ?? null);
    setDeleteTargetPath(null);
  };

  const saveContent = (content: string) => {
    if (!selectedFile) return;
    onChange(
      files.map((f) =>
        f.relative_path === selectedFile.relative_path ? { ...f, content } : f,
      ),
    );
  };
  const normalizedPromptPath = filePrompt ? normalizeSkillExtraPath(filePrompt.value) : "";
  const promptDuplicate = Boolean(
    normalizedPromptPath
    && files.some((f) =>
      f.relative_path === normalizedPromptPath
      && (filePrompt?.kind !== "rename" || f.relative_path !== filePrompt.originalPath),
    ),
  );
  const promptDisabled = !normalizedPromptPath
    || promptDuplicate
    || (filePrompt?.kind === "rename" && normalizedPromptPath === filePrompt.originalPath);
  const promptError = promptDuplicate ? "同名附加文件已存在" : null;

  return (
    <>
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
      <PromptDialog
        open={filePrompt !== null}
        title={filePrompt?.kind === "rename" ? "重命名附加文件" : "新建附加文件"}
        label="文件路径"
        value={filePrompt?.value ?? ""}
        confirmLabel={filePrompt?.kind === "rename" ? "重命名" : "新建"}
        disabled={promptDisabled}
        error={promptError}
        onValueChange={(value) => setFilePrompt((current) => current ? { ...current, value } : current)}
        onClose={() => setFilePrompt(null)}
        onConfirm={() => {
          if (!filePrompt) return;
          if (filePrompt.kind === "create") {
            confirmCreateFile(filePrompt.value);
            return;
          }
          confirmRenameFile(filePrompt.originalPath, filePrompt.value);
        }}
      />
      <DangerConfirmDialog
        open={deleteTargetPath !== null}
        title="删除附加文件"
        description={`确定要删除附加文件「${deleteTargetPath ?? ""}」吗？此操作不可撤销。`}
        confirmLabel="删除"
        onClose={() => setDeleteTargetPath(null)}
        onConfirm={confirmDeleteFile}
      />
    </>
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


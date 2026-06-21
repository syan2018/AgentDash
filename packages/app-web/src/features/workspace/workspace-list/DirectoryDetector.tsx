import { useMemo, useState, type ReactNode } from "react";
import type {
  BackendConfig,
  ProjectBackendAccess,
  WorkspaceDetectionResult,
} from "../../../types";
import { useWorkspaceStore } from "../../../stores/workspaceStore";
import { registerBackendWorkspaceInventory } from "../../../services/backendAccess";
import { DirectoryBrowserDialog } from "../directory-browser-dialog";
import { IDENTITY_KIND_LABELS, TERMS } from "../model/workspaceTerms";
import { detectionPrimaryText, type Feedback } from "./editorHelpers";

type DetectorMode = "fill-binding" | "register-inventory";

interface DirectoryDetectorProps {
  projectId: string;
  /** 选 backend 的候选列表（本机优先，回退到全部已授权）。 */
  detectBackends: BackendConfig[];
  accesses: ProjectBackendAccess[];
  /**
   * fill-binding：识别成功后由父级用结果填充创建表单（一步创建）。
   * register-inventory：识别成功后将目录登记为可选目录。
   */
  mode: DetectorMode;
  initialBackendId?: string;
  initialRootRef?: string;
  /** 识别成功回调，父级可据此填充表单或缓存结果。 */
  onDetected?: (result: WorkspaceDetectionResult) => void;
  /** 反馈消息上抛给父级统一渲染。 */
  onFeedback: (feedback: Feedback | null) => void;
  /** 登记可选目录成功后，请求父级刷新候选/库存。 */
  onInventoryRegistered?: () => void | Promise<void>;
  /** fill-binding 模式下，父级在识别结果区追加的主操作按钮（如「用这个目录创建 Workspace」）。 */
  renderPrimaryAction?: (result: WorkspaceDetectionResult) => ReactNode;
}

export function DirectoryDetector({
  projectId,
  detectBackends,
  accesses,
  mode,
  initialBackendId,
  initialRootRef,
  onDetected,
  onFeedback,
  onInventoryRegistered,
  renderPrimaryAction,
}: DirectoryDetectorProps) {
  const detectWorkspace = useWorkspaceStore((state) => state.detectWorkspace);
  const [detectBackendId, setDetectBackendId] = useState(initialBackendId ?? detectBackends[0]?.id ?? "");
  const [detectRootRef, setDetectRootRef] = useState(initialRootRef ?? "");
  const [detectionResult, setDetectionResult] = useState<WorkspaceDetectionResult | null>(null);
  const [isDetecting, setIsDetecting] = useState(false);
  const [isRegistering, setIsRegistering] = useState(false);
  const [isBrowseOpen, setIsBrowseOpen] = useState(false);

  const effectiveBackendId = useMemo(
    () => detectBackendId || detectBackends[0]?.id || "",
    [detectBackendId, detectBackends],
  );

  const runDetection = async (rootRefOverride?: string) => {
    const backendId = effectiveBackendId.trim();
    const rootRef = (rootRefOverride ?? detectRootRef).trim();
    if (!backendId || !rootRef) {
      onFeedback({ tone: "error", text: "请先选择已授权 Backend 并填写目录路径" });
      return;
    }
    setIsDetecting(true);
    onFeedback(null);
    try {
      const detected = await detectWorkspace(projectId, backendId, rootRef);
      if (!detected) return;
      setDetectionResult(detected);
      onDetected?.(detected);
      onFeedback(null);
    } finally {
      setIsDetecting(false);
    }
  };

  const handlePathCommitted = (path: string) => {
    const normalizedPath = path.trim();
    setDetectRootRef(normalizedPath);
    if (!normalizedPath) return;
    void runDetection(normalizedPath);
  };

  const handleRootBlur = () => {
    const trimmed = detectRootRef.trim();
    if (!trimmed || detectionResult?.binding.root_ref === trimmed) return;
    handlePathCommitted(trimmed);
  };

  const handleRegisterInventory = async () => {
    const backendId = effectiveBackendId.trim();
    const rootRef = (detectionResult?.binding.root_ref ?? detectRootRef).trim();
    if (!backendId || !rootRef) {
      onFeedback({ tone: "error", text: "请先选择已授权 Backend 并识别或填写目录" });
      return;
    }
    const access = accesses.find((item) => item.backend_id === backendId && item.status === "active");
    if (!access) {
      onFeedback({ tone: "error", text: "当前 Project 尚未授权这个 Backend，无法添加可选目录" });
      return;
    }

    setIsRegistering(true);
    onFeedback(null);
    try {
      await registerBackendWorkspaceInventory(projectId, access.id, { root_ref: rootRef });
      await onInventoryRegistered?.();
      onFeedback({ tone: "success", text: "已登记为可选目录，可在上方确认为目录绑定" });
    } catch (registerError) {
      onFeedback({ tone: "error", text: (registerError as Error).message });
    } finally {
      setIsRegistering(false);
    }
  };

  return (
    <div className="space-y-3">
      <div className="grid gap-3 md:grid-cols-[200px_minmax(0,1fr)]">
        <select
          value={effectiveBackendId}
          onChange={(event) => setDetectBackendId(event.target.value)}
          className="agentdash-form-select"
        >
          <option value="">选择已授权 Backend</option>
          {detectBackends.map((backend) => (
            <option key={backend.id} value={backend.id}>
              {backend.name} {backend.backend_type === "local" ? "(本机)" : "(远程)"}
            </option>
          ))}
        </select>

        <div className="flex gap-1.5">
          <input
            value={detectRootRef}
            onChange={(event) => setDetectRootRef(event.target.value)}
            onBlur={handleRootBlur}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                handlePathCommitted(detectRootRef);
              }
            }}
            placeholder="选择或填写 backend 上的目录"
            className="agentdash-form-input min-w-0 flex-1"
          />
          <button
            type="button"
            onClick={() => setIsBrowseOpen(true)}
            disabled={!effectiveBackendId}
            className="shrink-0 rounded-[8px] border border-border bg-background px-2.5 py-2 text-xs text-muted-foreground hover:bg-secondary disabled:cursor-not-allowed disabled:opacity-40"
          >
            浏览
          </button>
        </div>
      </div>
      <p className="text-[11px] text-muted-foreground">
        {isDetecting ? "正在识别目录..." : "选择目录后会自动识别；手动填写时按 Enter 或离开输入框确认。"}
      </p>

      {detectBackends.length === 0 && (
        <p className="rounded-[8px] border border-dashed border-border px-3 py-3 text-xs text-muted-foreground">
          当前没有已授权且在线的 Backend。请先在 Backend Access 中授权本机 Backend。
        </p>
      )}

      {detectionResult && (
        <div className="flex flex-wrap items-start justify-between gap-3 rounded-[8px] border border-border bg-background px-3 py-3 text-xs text-muted-foreground">
          <div className="min-w-0 flex-1">
            <p>
              识别结果：{IDENTITY_KIND_LABELS[detectionResult.identity_kind]}
              <span className="text-muted-foreground/60"> · </span>
              <span className="font-mono text-foreground">
                {detectionPrimaryText(detectionResult)}
              </span>
            </p>
            <p className="mt-1">
              解析目录：<span className="font-mono text-foreground">{detectionResult.binding.root_ref}</span>
            </p>
            {mode === "fill-binding" && detectionResult.matched_workspace_ids.length > 0 && (
              <p className="mt-1 text-warning">
                检测到 {detectionResult.matched_workspace_ids.length} 个可能重复的 Workspace，请确认后再创建。
              </p>
            )}
            {detectionResult.warnings.map((warning) => (
              <p key={warning} className="mt-1 text-warning">{warning}</p>
            ))}
          </div>
          <div className="flex shrink-0 flex-col gap-2">
            {mode === "fill-binding" && renderPrimaryAction?.(detectionResult)}
            <button
              type="button"
              onClick={() => void handleRegisterInventory()}
              disabled={isRegistering}
              className="agentdash-button-secondary disabled:cursor-not-allowed disabled:opacity-50"
            >
              {isRegistering
                ? "登记中..."
                : mode === "fill-binding"
                  ? `仅登记为${TERMS.inventory}`
                  : `登记为${TERMS.inventory}`}
            </button>
          </div>
        </div>
      )}

      <DirectoryBrowserDialog
        open={isBrowseOpen}
        backendId={effectiveBackendId}
        initialPath={detectRootRef || undefined}
        onSelect={handlePathCommitted}
        onClose={() => setIsBrowseOpen(false)}
      />
    </div>
  );
}

/* eslint-disable react-refresh/only-export-components */
import { useCallback } from "react";
import { VfsBrowserPanel } from "../../vfs";
import { useWorkspaceData } from "../workspace-data-context";
import type { TabContentRenderProps, TabTypeDescriptor } from "../tab-type-registry";
import { useWorkspaceTabStore } from "../../../stores/workspaceTabStore";
import { VfsIcon } from "./icons";

/**
 * VFS URI 格式：  `{mountId}://{filePath}`
 * 例：  `main://README.md`、 `cvs-test-canvas-001://src/main.tsx`
 * 无文件时：  `main://`
 * 默认（无 mount 选择）：  `vfs://`
 */
const FALLBACK_SCHEME = "vfs://";

function parseMountUri(uri: string): { mountId?: string; path?: string } | null {
  const schemeEnd = uri.indexOf("://");
  if (schemeEnd < 0) return null;

  const mountId = uri.slice(0, schemeEnd);
  if (mountId === "vfs") return {};

  const path = uri.slice(schemeEnd + 3) || undefined;
  return { mountId, path };
}

function buildMountUri(mountId: string, filePath?: string | null): string {
  if (!mountId) return FALLBACK_SCHEME;
  return filePath ? `${mountId}://${filePath}` : `${mountId}://`;
}

function VfsTabContent({ uri, tabId }: TabContentRenderProps) {
  const { vfs, runtimeSurface } = useWorkspaceData();
  const parsed = parseMountUri(uri);

  const hasMounts =
    (vfs && vfs.mounts.length > 0) ||
    (runtimeSurface && runtimeSurface.mounts.length > 0);

  const handleNavigate = useCallback(
    (mountId: string, filePath: string | null) => {
      const newUri = buildMountUri(mountId, filePath);
      useWorkspaceTabStore.getState().updateTabUri(tabId, newUri);
    },
    [tabId],
  );

  if (!hasMounts) {
    return (
      <div className="flex h-full min-h-[200px] items-center justify-center px-6">
        <p className="text-center text-sm text-muted-foreground">
          当前会话没有挂载的地址空间。
        </p>
      </div>
    );
  }

  return (
    <VfsBrowserPanel
      surface={runtimeSurface}
      vfs={vfs}
      initialMountId={parsed?.mountId}
      onNavigate={handleNavigate}
    />
  );
}

export const vfsTabType: TabTypeDescriptor = {
  typeId: "vfs",
  label: "地址空间",
  icon: VfsIcon,
  allowMultiple: true,
  pinned: false,

  renderContent: (props) => <VfsTabContent {...props} />,

  resolveTitle: (uri) => {
    const parsed = parseMountUri(uri);
    if (parsed?.path) {
      const filename = parsed.path.split("/").pop() ?? parsed.path;
      return filename;
    }
    if (parsed?.mountId) return parsed.mountId;
    return "地址空间";
  },

  parseUri: (uri) => {
    const parsed = parseMountUri(uri);
    return parsed as Record<string, string> | null;
  },

  buildUri: (params) => {
    const mountId = params?.mountId;
    const path = params?.path;
    if (!mountId) return FALLBACK_SCHEME;
    return buildMountUri(mountId, path);
  },

  defaultUri: FALLBACK_SCHEME,
  menuOrder: 20,
};

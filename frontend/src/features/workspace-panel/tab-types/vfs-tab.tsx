/* eslint-disable react-refresh/only-export-components */
import { useCallback } from "react";
import { VfsBrowserPanel } from "../../vfs";
import { useWorkspaceData } from "../workspace-data-context";
import type { TabContentRenderProps, TabTypeDescriptor } from "../tab-type-registry";
import { useWorkspaceTabStore } from "../../../stores/workspaceTabStore";
import { VfsIcon } from "./icons";

const SCHEME = "vfs://";

function parseVfsUri(uri: string): { surfaceRef?: string; mountId?: string; path?: string } | null {
  if (!uri.startsWith(SCHEME)) return null;
  const rest = uri.slice(SCHEME.length);
  const qIdx = rest.indexOf("?");
  const pathPart = qIdx >= 0 ? rest.slice(0, qIdx) : rest;
  const queryPart = qIdx >= 0 ? rest.slice(qIdx + 1) : "";
  const parts = pathPart.split("/");

  const params = new URLSearchParams(queryPart);
  return {
    surfaceRef: parts[0] || undefined,
    mountId: parts[1] || undefined,
    path: params.get("path") || undefined,
  };
}

function VfsTabContent({ uri, tabId }: TabContentRenderProps) {
  const { vfs, runtimeSurface } = useWorkspaceData();
  const parsed = parseVfsUri(uri);
  const surfaceRef = runtimeSurface?.surface_ref ?? "default";

  const hasMounts =
    (vfs && vfs.mounts.length > 0) ||
    (runtimeSurface && runtimeSurface.mounts.length > 0);

  const handleNavigate = useCallback(
    (mountId: string, filePath: string | null) => {
      let newUri = `${SCHEME}${surfaceRef}/${mountId}`;
      if (filePath) newUri += `?path=${encodeURIComponent(filePath)}`;
      useWorkspaceTabStore.getState().updateTabUri(tabId, newUri);
    },
    [tabId, surfaceRef],
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
    const parsed = parseVfsUri(uri);
    if (parsed?.path) {
      const filename = parsed.path.split("/").pop() ?? parsed.path;
      return filename;
    }
    if (parsed?.mountId && parsed.mountId !== "default") return `VFS: ${parsed.mountId}`;
    return "地址空间";
  },

  parseUri: (uri) => {
    const parsed = parseVfsUri(uri);
    return parsed as Record<string, string> | null;
  },

  buildUri: (params) => {
    const surfaceRef = params?.surfaceRef;
    const mountId = params?.mountId;
    const path = params?.path;
    let result = SCHEME;
    if (surfaceRef && mountId) result = `${SCHEME}${surfaceRef}/${mountId}`;
    else if (surfaceRef) result = `${SCHEME}${surfaceRef}`;
    else result = `${SCHEME}default`;
    if (path) result += `?path=${encodeURIComponent(path)}`;
    return result;
  },

  defaultUri: `${SCHEME}default`,
  menuOrder: 20,
};

export interface VfsMountBrowsingPolicy {
  id: string;
  provider: string;
  browsable: boolean;
  backend_online?: boolean | null;
}

export interface VfsMountSelectionPolicy {
  id: string;
  provider: string;
  browsable?: boolean;
  backend_online?: boolean | null;
}

export interface VfsMountBackendPolicy extends VfsMountSelectionPolicy {
  backend_id?: string | null;
  display_name?: string | null;
}

export interface VfsMountSelectionOptions {
  initialMountId?: string;
  defaultMountId?: string | null;
}

export interface VfsBackendTargetSelection {
  mountId: string;
  backend_id: string;
  label: string;
  online: boolean;
}

export function isVfsMountBrowsable(
  mount: { provider: string; backend_online?: boolean | null },
): boolean {
  return mount.provider !== "relay_fs" || !("backend_online" in mount) || mount.backend_online !== false;
}

function resolveMountBrowsable(mount: VfsMountSelectionPolicy): boolean {
  return typeof mount.browsable === "boolean" ? mount.browsable : isVfsMountBrowsable(mount);
}

function isPreferredAutoBrowseMount(
  mount: Pick<Required<VfsMountBrowsingPolicy>, "id" | "provider" | "browsable">,
): boolean {
  if (!mount.browsable) return false;
  return [
    "relay_fs",
    "inline_fs",
    "lifecycle_vfs",
    "canvas_fs",
    "skill_asset_fs",
  ].includes(mount.provider);
}

function findPreferredAutoBrowseMount<
  T extends Pick<Required<VfsMountBrowsingPolicy>, "id" | "provider" | "browsable">,
>(
  mounts: T[],
): T | null {
  return mounts.find((mount) => mount.id === "main" && mount.provider === "relay_fs" && mount.browsable)
    ?? mounts.find((mount) => mount.provider === "relay_fs" && mount.browsable)
    ?? mounts.find(isPreferredAutoBrowseMount)
    ?? null;
}

export function resolveDefaultMountId<
  T extends Pick<Required<VfsMountBrowsingPolicy>, "id" | "provider" | "browsable">,
>(
  mounts: T[],
  initialMountId?: string,
  defaultMountId?: string | null,
): string | null {
  const initialMount = initialMountId
    ? mounts.find((item) => item.id === initialMountId && item.browsable)
    : null;
  if (initialMount) return initialMount.id;

  const defaultMount = defaultMountId
    ? mounts.find((item) => item.id === defaultMountId && item.browsable)
    : null;
  if (defaultMount && isPreferredAutoBrowseMount(defaultMount)) {
    return defaultMount.id;
  }

  return findPreferredAutoBrowseMount(mounts)?.id
    ?? defaultMount?.id
    ?? mounts.find((mount) => mount.browsable)?.id
    ?? mounts[0]?.id
    ?? null;
}

export function selectDefaultVfsMount<T extends VfsMountSelectionPolicy>(
  mounts: T[],
  options: VfsMountSelectionOptions = {},
): T | null {
  const policyMounts = mounts.map((mount) => ({
    id: mount.id,
    provider: mount.provider,
    browsable: resolveMountBrowsable(mount),
  }));
  const selectedId = resolveDefaultMountId(
    policyMounts,
    options.initialMountId,
    options.defaultMountId,
  );
  return selectedId ? mounts.find((mount) => mount.id === selectedId) ?? null : null;
}

export function selectVfsBackendTarget<T extends VfsMountBackendPolicy>(
  mounts: T[],
  options: VfsMountSelectionOptions = {},
): VfsBackendTargetSelection | null {
  const backendMounts = mounts.filter((mount) => {
    const backendId = mount.backend_id?.trim() ?? "";
    return backendId.length > 0 && resolveMountBrowsable(mount);
  });
  const selected = selectDefaultVfsMount(backendMounts, options);
  const backendId = selected?.backend_id?.trim() ?? "";
  if (!selected || !backendId) return null;
  return {
    mountId: selected.id,
    backend_id: backendId,
    label: selected.display_name?.trim() || backendId,
    online: selected.backend_online !== false,
  };
}

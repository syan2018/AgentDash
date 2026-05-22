export interface VfsMountBrowsingPolicy {
  id: string;
  provider: string;
  browsable: boolean;
  backend_online?: boolean | null;
}

export function isVfsMountBrowsable(mount: { provider: string; backend_online?: boolean | null }): boolean {
  return mount.provider !== "relay_fs" || !("backend_online" in mount) || mount.backend_online !== false;
}

function isPreferredAutoBrowseMount(mount: Pick<VfsMountBrowsingPolicy, "id" | "provider" | "browsable">): boolean {
  if (!mount.browsable) return false;
  return [
    "relay_fs",
    "inline_fs",
    "lifecycle_vfs",
    "canvas_fs",
    "skill_asset_fs",
  ].includes(mount.provider);
}

function findPreferredAutoBrowseMount<T extends Pick<VfsMountBrowsingPolicy, "id" | "provider" | "browsable">>(
  mounts: T[],
): T | null {
  return mounts.find((mount) => mount.id === "main" && mount.provider === "relay_fs" && mount.browsable)
    ?? mounts.find((mount) => mount.provider === "relay_fs" && mount.browsable)
    ?? mounts.find(isPreferredAutoBrowseMount)
    ?? null;
}

export function resolveDefaultMountId<T extends Pick<VfsMountBrowsingPolicy, "id" | "provider" | "browsable">>(
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

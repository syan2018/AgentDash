export interface VfsMountBrowsingPolicy {
  id: string;
  provider: string;
  browsable: boolean;
  backend_online?: boolean | null;
}

export function isVfsMountBrowsable(mount: { provider: string; backend_online?: boolean | null }): boolean {
  return mount.provider !== "relay_fs" || !("backend_online" in mount) || mount.backend_online !== false;
}

export function resolveDefaultMountId<T extends Pick<VfsMountBrowsingPolicy, "id" | "browsable">>(
  mounts: T[],
  initialMountId?: string,
  defaultMountId?: string | null,
): string | null {
  const preferred = [initialMountId, defaultMountId].filter((id): id is string => Boolean(id));
  for (const id of preferred) {
    const mount = mounts.find((item) => item.id === id && item.browsable);
    if (mount) return mount.id;
  }
  return mounts.find((mount) => mount.browsable)?.id ?? mounts[0]?.id ?? null;
}

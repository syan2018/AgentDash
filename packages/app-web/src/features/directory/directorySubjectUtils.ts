import type { DirectoryGroup, DirectoryUser } from "../../types";

export type DirectoryGroupSummary = Pick<
  DirectoryGroup,
  "group_id" | "display_name" | "path" | "provider" | "source"
>;

export function resolveUserLabel(user: DirectoryUser): string {
  return user.display_name?.trim() || user.email?.trim() || user.subject || user.user_id;
}

export function resolveGroupLabel(group: DirectoryGroupSummary): string {
  return group.display_name?.trim() || group.path?.trim() || group.group_id;
}

export function mergeDirectoryUsers(current: DirectoryUser[], incoming: DirectoryUser[]): DirectoryUser[] {
  const merged = new Map(current.map((user) => [user.user_id, user]));
  for (const user of incoming) merged.set(user.user_id, user);
  return Array.from(merged.values()).sort((left, right) =>
    resolveUserLabel(left).localeCompare(resolveUserLabel(right)),
  );
}

export function mergeDirectoryGroups(
  current: DirectoryGroupSummary[],
  incoming: DirectoryGroupSummary[],
): DirectoryGroupSummary[] {
  const merged = new Map(current.map((group) => [group.group_id, group]));
  for (const group of incoming) merged.set(group.group_id, { ...merged.get(group.group_id), ...group });
  return Array.from(merged.values()).sort((left, right) =>
    resolveGroupLabel(left).localeCompare(resolveGroupLabel(right)),
  );
}

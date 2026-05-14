export const FILE_PILL_CLASS = "agentdash-file-pill";
export const FILE_PILL_BADGE_CLASS = "agentdash-file-pill-badge";
export const FILE_PILL_LABEL_CLASS = "agentdash-file-pill-label";
export const FILE_PILL_REMOVE_CLASS = "agentdash-file-pill-remove";

export function toFileUri(relPath: string): string {
  const normalized = relPath.replace(/\\/g, "/").replace(/^\/+/, "");
  return `file:///${normalized}`;
}

export function getDisplayFileName(relPath: string): string {
  return relPath.replace(/\\/g, "/").split("/").pop() || relPath;
}

export function getFileKindLabel(relPath: string): string {
  const normalized = relPath.replace(/\\/g, "/");
  const fileName = normalized.split("/").pop()?.toLowerCase() ?? "";
  const ext = fileName.split(".").pop()?.toUpperCase();

  if (fileName === "dockerfile") return "DOCK";
  if (fileName === ".env" || fileName.startsWith(".env.")) return "ENV";
  if (fileName === "package.json") return "PKG";
  if (fileName === "tsconfig.json") return "TSC";
  if (fileName === "readme.md") return "MD";
  if (!ext || ext === fileName.toUpperCase()) return "FILE";

  return ext.slice(0, 4);
}

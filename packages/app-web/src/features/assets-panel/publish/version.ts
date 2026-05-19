/**
 * "1.0.0" → "1.0.1"；非 semver 字符串 fallback 到 "${input}.1"。
 *
 * 不强求 semver 严谨，只是给个合理的下一个版本号建议，用户随时可改。
 */
export function suggestNextVersion(prev: string): string {
  const trimmed = prev.trim();
  const match = trimmed.match(/^(\d+)\.(\d+)\.(\d+)(.*)$/);
  if (match) {
    const major = match[1];
    const minor = match[2];
    const patch = Number.parseInt(match[3], 10);
    const suffix = match[4] ?? "";
    return `${major}.${minor}.${patch + 1}${suffix}`;
  }
  return `${trimmed}.1`;
}

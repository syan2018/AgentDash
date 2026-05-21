import { useState } from "react";

interface UserAvatarProps {
  avatarUrl?: string | null;
  fallback: string;
  sizeClassName?: string;
  className?: string;
}

function normalizeAvatarUrl(value?: string | null): string | null {
  const trimmed = value?.trim();
  if (!trimmed) return null;
  if (trimmed.startsWith("/")) return trimmed;
  if (trimmed.startsWith("//")) return `https:${trimmed}`;

  try {
    const url = new URL(trimmed);
    return url.protocol === "http:" || url.protocol === "https:" ? trimmed : null;
  } catch {
    return null;
  }
}

export function UserAvatar({
  avatarUrl,
  fallback,
  sizeClassName = "h-7 w-7",
  className = "",
}: UserAvatarProps) {
  const [failedAvatarUrl, setFailedAvatarUrl] = useState<string | null>(null);
  const normalizedAvatarUrl = normalizeAvatarUrl(avatarUrl);
  const usableAvatar =
    normalizedAvatarUrl && normalizedAvatarUrl !== failedAvatarUrl ? normalizedAvatarUrl : null;
  const roundedShapeClass = "rounded-full";
  const initial = fallback.trim().charAt(0).toUpperCase() || "?";

  if (usableAvatar) {
    return (
      <img
        src={usableAvatar}
        alt=""
        className={`${sizeClassName} shrink-0 ${roundedShapeClass} bg-secondary object-cover ${className}`}
        referrerPolicy="no-referrer"
        onError={() => setFailedAvatarUrl(usableAvatar)}
      />
    );
  }

  return (
    <span
      className={`${sizeClassName} flex shrink-0 items-center justify-center ${roundedShapeClass} bg-secondary text-xs font-semibold text-foreground ${className}`}
    >
      {initial}
    </span>
  );
}

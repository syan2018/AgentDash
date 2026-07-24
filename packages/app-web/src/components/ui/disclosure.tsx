import type { ButtonHTMLAttributes, ReactNode } from "react";

export function DisclosureChevron({
  expanded,
  className = "",
}: {
  expanded: boolean;
  className?: string;
}) {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 12 12"
      fill="none"
      className={`block shrink-0 transition-transform ${expanded ? "rotate-90" : ""} ${className}`}
      aria-hidden="true"
    >
      <path
        d="M4.5 2.5 8 6 4.5 9.5"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function DisclosureRow({
  expanded,
  children,
  className = "",
  ...buttonProps
}: {
  expanded: boolean;
  children: ReactNode;
} & Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children">) {
  return (
    <button
      type="button"
      {...buttonProps}
      aria-expanded={expanded}
      className={`flex w-full items-center gap-2 text-left leading-4 ${className}`}
    >
      <span className="flex size-4 shrink-0 items-center justify-center text-muted-foreground/40">
        <DisclosureChevron expanded={expanded} />
      </span>
      {children}
    </button>
  );
}

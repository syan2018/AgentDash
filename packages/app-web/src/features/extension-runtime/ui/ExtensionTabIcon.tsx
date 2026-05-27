export function ExtensionTabIcon({ className = "h-3.5 w-3.5" }: { className?: string }) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
    >
      <path d="M8 3h8v4h4v8h-4v6H8v-6H4V7h4z" />
      <path d="M9 12h6" />
      <path d="M12 9v6" />
    </svg>
  );
}

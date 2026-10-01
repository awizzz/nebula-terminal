import type { ProfileKind } from "../types";

/** Small monochrome-friendly glyphs that identify a shell in tabs and menus. */
export default function ProfileIcon({ kind, size = 16 }: { kind: ProfileKind; size?: number }) {
  const common = { width: size, height: size, viewBox: "0 0 16 16", "aria-hidden": true, className: "profile-icon" } as const;

  if (kind === "pwsh" || kind === "powershell") {
    const fill = kind === "pwsh" ? "#2f6fdc" : "#245aa8";
    return (
      <svg {...common}>
        <path d="M3.2 2.5h10.6a.9.9 0 0 1 .87 1.12l-2.1 8.76a1.4 1.4 0 0 1-1.36 1.07H.6a.9.9 0 0 1-.87-1.12l2.1-8.76A1.4 1.4 0 0 1 3.2 2.5Z" fill={fill} transform="translate(.6 0)" />
        <path d="m5 5.6 3 2.2-3.9 2.5" fill="none" stroke="#fff" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
        <path d="M8.4 10.6h3" stroke="#fff" strokeWidth="1.3" strokeLinecap="round" />
      </svg>
    );
  }

  if (kind === "cmd") {
    return (
      <svg {...common}>
        <rect x="1" y="2" width="14" height="12" rx="2" fill="#2b2d31" stroke="#5c6068" strokeWidth=".8" />
        <path d="m4 6 2.4 2L4 10" fill="none" stroke="#e6e6e6" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
        <path d="M8 10.5h4" stroke="#e6e6e6" strokeWidth="1.3" strokeLinecap="round" />
      </svg>
    );
  }

  if (kind === "gitbash") {
    return (
      <svg {...common}>
        <rect x="2.4" y="2.4" width="11.2" height="11.2" rx="2.2" transform="rotate(45 8 8)" fill="#e5734a" />
        <circle cx="6.2" cy="6.2" r="1.15" fill="#fff" />
        <circle cx="9.8" cy="9.8" r="1.15" fill="#fff" />
        <circle cx="6.2" cy="10.4" r="1.15" fill="#fff" />
        <path d="M6.2 6.2v4.2M6.2 6.2l3.6 3.6" stroke="#fff" strokeWidth="1" />
      </svg>
    );
  }

  return (
    <svg {...common}>
      <circle cx="8" cy="8" r="6.6" fill="#e0a040" />
      <path d="M5 6.2 7 8l-2 1.8" fill="none" stroke="#2a1d08" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M8.2 10.2h2.8" stroke="#2a1d08" strokeWidth="1.3" strokeLinecap="round" />
    </svg>
  );
}

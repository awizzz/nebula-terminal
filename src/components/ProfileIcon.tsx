import { readableOn } from "../color";
import type { ProfileKind } from "../types";

const HEX_COLOR = /^#[0-9a-f]{6}$/i;

/**
 * Small monochrome-friendly glyphs that identify a shell in tabs and menus. Custom
 * profiles are drawn in their own accent color.
 */
export default function ProfileIcon({ kind, accent, size = 16 }: { kind: ProfileKind; accent?: string; size?: number }) {
  const common = { width: size, height: size, viewBox: "0 0 16 16", "aria-hidden": true, className: "profile-icon" } as const;

  if (kind === "custom") {
    const fill = accent && HEX_COLOR.test(accent) ? accent : "#9aa3ab";
    const ink = readableOn(fill);
    return (
      <svg {...common}>
        <rect x="1" y="2" width="14" height="12" rx="2.4" fill={fill} />
        <path d="m4.2 6 2.4 2-2.4 2" fill="none" stroke={ink} strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
        <path d="M8.2 10.4h3.6" stroke={ink} strokeWidth="1.3" strokeLinecap="round" />
      </svg>
    );
  }

  if (kind === "ssh") {
    return (
      <svg {...common}>
        <rect x="0.5" y="0.5" width="15" height="15" rx="3.5" fill="#2b6b62" />
        <g fill="none" stroke="#eef6f4" strokeWidth="1.05">
          <circle cx="8" cy="8" r="4.7" />
          <ellipse cx="8" cy="8" rx="2" ry="4.7" />
          <path d="M3.3 8h9.4" strokeLinecap="round" />
        </g>
      </svg>
    );
  }

  if (kind === "nebula") {
    return (
      <svg {...common}>
        <rect width="16" height="16" rx="3" fill="#17181c" />
        {/* The app icon's dotted prompt, cut down to five dots for this size. */}
        <g fill="#e9a439">
          <circle cx="4.6" cy="3.6" r="1.7" />
          <circle cx="8" cy="5.8" r="1.25" />
          <circle cx="11.4" cy="8" r="2.05" />
          <circle cx="8" cy="10.2" r="1.25" />
          <circle cx="4.6" cy="12.4" r="1.7" />
        </g>
      </svg>
    );
  }

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

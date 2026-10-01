import { useEffect, useRef } from "react";
import type { TerminalProfile } from "../types";

interface NewTabMenuProps {
  open: boolean;
  defaultProfileId?: string;
  profiles: TerminalProfile[];
  onPick: (profileId: string) => void;
  onClose: () => void;
}

export default function NewTabMenu({ open, defaultProfileId, profiles, onPick, onClose }: NewTabMenuProps) {
  const ref = useRef<HTMLDivElement | null>(null);
  const firstEnabledRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (!open) return;
    requestAnimationFrame(() => firstEnabledRef.current?.focus());
    const close = (event: MouseEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) onClose();
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", closeOnEscape);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", closeOnEscape);
    };
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className="new-tab-menu" ref={ref} role="menu" aria-label="New terminal profile">
      <div className="new-tab-menu__header">New terminal</div>
      {profiles.map((profile, index) => (
        <button
          ref={profile.available && (index === profiles.findIndex((item) => item.available)) ? firstEnabledRef : undefined}
          key={profile.id}
          type="button"
          role="menuitem"
          disabled={!profile.available}
          onClick={() => {
            onPick(profile.id);
            onClose();
          }}
        >
          <span className="profile-color" style={{ background: profile.accent }} />
          <span className="new-tab-menu__label">
            <strong>{profile.name}</strong>
            <small>{profile.available ? profile.executable ?? "Built in" : "Not detected"}</small>
          </span>
          {profile.id === defaultProfileId && <span className="new-tab-menu__badge">Default</span>}
        </button>
      ))}
    </div>
  );
}

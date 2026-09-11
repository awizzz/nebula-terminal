import { useEffect, useRef } from "react";
import type { TerminalProfile } from "../types";

interface NewTabMenuProps {
  open: boolean;
  profiles: TerminalProfile[];
  onPick: (profileId: string) => void;
  onClose: () => void;
}

export default function NewTabMenu({ open, profiles, onPick, onClose }: NewTabMenuProps) {
  const ref = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) onClose();
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className="new-tab-menu" ref={ref} role="menu" aria-label="New terminal profile">
      <div className="new-tab-menu__header">New terminal</div>
      {profiles.map((profile) => (
        <button
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
          {profile.id === "nebula" && <span className="new-tab-menu__badge">Default</span>}
        </button>
      ))}
    </div>
  );
}

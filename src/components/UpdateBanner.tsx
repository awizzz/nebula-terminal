import { useState } from "react";
import { ArrowUpRight, Download, X } from "lucide-react";
import { openExternal } from "../external";
import { installUpdate, type UpdateInfo } from "../updates";

interface UpdateBannerProps {
  update: UpdateInfo;
  onDismiss: () => void;
}

/** A quiet card in the corner: a new version is out, install it when you like. */
export default function UpdateBanner({ update, onDismiss }: UpdateBannerProps) {
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const manual = update.install === "manual";

  const install = () => {
    setInstalling(true);
    setError(null);
    installUpdate().catch((reason) => {
      setInstalling(false);
      setError(reason instanceof Error ? reason.message : String(reason));
    });
  };

  return (
    <aside className="update" role="status" aria-live="polite">
      <div className="update__header">
        <div className="update__icon" aria-hidden="true"><Download size={16} /></div>
        <div>
          <p className="update__title">Nebula Terminal {update.version} is available</p>
          <p className="update__text">
            {error ?? (installing
              ? "Downloading and checking the update. The app restarts when it's installed."
              : `You have ${update.current}.`)}
          </p>
        </div>
      </div>
      <div className="update__actions">
        <button className="button button--link" type="button" onClick={() => openExternal(update.url)}>
          What's new<ArrowUpRight size={13} />
        </button>
        <span className="update__spacer" />
        {!installing && <button className="button" type="button" onClick={onDismiss}>Later</button>}
        {manual
          ? <button className="button button--primary" type="button" onClick={() => openExternal(update.url)}>Download</button>
          : <button className={`button button--primary ${installing ? "is-busy" : ""}`} type="button" disabled={installing} onClick={install}>
              {installing ? "Installing…" : error ? "Try again" : "Install and restart"}
            </button>}
      </div>
      {!installing && (
        <button className="update__close icon-button" type="button" aria-label="Dismiss" onClick={onDismiss}><X size={14} /></button>
      )}
    </aside>
  );
}

declare global {
  interface Window {
    /** "quick" in the quick terminal (src-tauri/src/quick.rs), "tabs" in windows opened
     * for tabs dragged out of another (windows.rs), unset in the main window. */
    __NEBULA_WINDOW__?: string;
    /** What a "tabs" window opens with, to ask windows.rs for. */
    __NEBULA_TRANSFER__?: string | null;
  }
}

/**
 * The quick terminal runs the same page in a window of its own that drops down from the
 * top of the screen. It keeps its tabs while hidden, but has no session to restore and
 * leaves Windows integration (Explorer, updates, the default terminal) to the main window.
 */
export const isQuickTerminal = typeof window !== "undefined" && window.__NEBULA_WINDOW__ === "quick";

/** A window opened for tabs moved out of another one, or restored from the last session. */
export const isTabsWindow = typeof window !== "undefined" && window.__NEBULA_WINDOW__ === "tabs";

export const transferId = typeof window !== "undefined" ? window.__NEBULA_TRANSFER__ ?? undefined : undefined;

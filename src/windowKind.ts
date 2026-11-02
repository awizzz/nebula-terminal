declare global {
  interface Window {
    /** "quick" in the quick terminal (src-tauri/src/quick.rs), unset in the main window. */
    __NEBULA_WINDOW__?: string;
  }
}

/**
 * The quick terminal runs the same page in a window of its own that drops down from the
 * top of the screen. It keeps its tabs while hidden, but has no session to restore and
 * leaves Windows integration (Explorer, updates, the default terminal) to the main window.
 */
export const isQuickTerminal = typeof window !== "undefined" && window.__NEBULA_WINDOW__ === "quick";

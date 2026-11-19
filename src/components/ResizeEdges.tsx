import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";

type Direction = "North" | "South" | "East" | "West" | "NorthEast" | "NorthWest" | "SouthEast" | "SouthWest";

const EDGES: Direction[] = ["North", "South", "East", "West", "NorthEast", "NorthWest", "SouthEast", "SouthWest"];

/**
 * Borders to resize the window by. Windows draws them around a window without a title
 * bar of its own; Linux window managers don't, so the page does.
 */
export default function ResizeEdges() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const win = getCurrentWindow();
    let disposed = false;
    const update = () => void win.isMaximized().then((value) => !disposed && setMaximized(value)).catch(() => undefined);
    update();
    const unlisten = win.onResized(update);
    return () => {
      disposed = true;
      void unlisten.then((stop) => stop());
    };
  }, []);

  if (maximized) return null;
  return (
    <div className="resize-edges" aria-hidden="true">
      {EDGES.map((edge) => (
        <div
          key={edge}
          className={`resize-edges__edge resize-edges__edge--${edge.toLowerCase()}`}
          onMouseDown={(event) => {
            if (event.button !== 0) return;
            event.preventDefault();
            void getCurrentWindow().startResizeDragging(edge);
          }}
        />
      ))}
    </div>
  );
}

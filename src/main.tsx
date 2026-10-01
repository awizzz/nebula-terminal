import React from "react";
import ReactDOM from "react-dom/client";
import "@xterm/xterm/css/xterm.css";
import "./styles.css";
import App from "./App";

// xterm caches glyphs as it draws them, so the icon font must be ready before the
// first terminal renders. Never wait more than a moment for it.
const symbols = document.fonts.load('16px "Nebula Symbols"', "\uf07b").catch(() => undefined);
const timeout = new Promise((resolve) => setTimeout(resolve, 1500));

void Promise.race([symbols, timeout]).then(() => {
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
});

# Icon

The Nebula prompt `❯`, drawn as a cluster of dots in amber `#e9a439` on `#17181c`.

| File | For |
| --- | --- |
| `icon.svg` | 64 px and up: the app icon, the About page, the website. |
| `icon-medium.svg` | 28 to 48 px: seven dots. |
| `icon-small.svg` | 16 to 24 px: five dots, the size of a tab. The Nebula tab draws the same shape in `src/components/ProfileIcon.tsx`. |

`src-tauri/icons/icon.ico` holds 16, 20 and 24 px from `icon-small.svg`, 32, 40 and 48 px from `icon-medium.svg`, and 64 and 256 px from `icon.svg`. `src-tauri/icons/icon.png` is `icon.svg` at 512 px. Render them with a browser engine (Chromium) for clean edges, then pack the PNGs into the `.ico`.

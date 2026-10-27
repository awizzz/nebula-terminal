// Builds src/assets/color-schemes.json, the theme gallery, from a checkout of
// https://github.com/mbadolato/iTerm2-Color-Schemes (its windowsterminal folder):
//
//   git clone --filter=blob:none --sparse --depth 1 https://github.com/mbadolato/iTerm2-Color-Schemes.git
//   git -C iTerm2-Color-Schemes sparse-checkout set windowsterminal
//   node scripts/build-color-schemes.mjs iTerm2-Color-Schemes
//
// Each entry is [name, background, foreground, cursor or "", the 16 ANSI colors], hex
// without "#". src/themeImport.ts turns them into themes with the same rules as an import.

import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const source = process.argv[2];
if (!source) {
  console.error("usage: node scripts/build-color-schemes.mjs <iTerm2-Color-Schemes checkout>");
  process.exit(1);
}

const ANSI = [
  "black", "red", "green", "yellow", "blue", "purple", "cyan", "white",
  "brightBlack", "brightRed", "brightGreen", "brightYellow", "brightBlue", "brightPurple", "brightCyan", "brightWhite",
];
const hex = (value) => (typeof value === "string" && /^#[0-9a-f]{6}$/i.test(value) ? value.slice(1).toLowerCase() : null);

const folder = join(source, "windowsterminal");
const entries = [];
const skipped = [];
for (const file of readdirSync(folder).filter((name) => name.endsWith(".json")).sort()) {
  const scheme = JSON.parse(readFileSync(join(folder, file), "utf8"));
  const colors = [scheme.background, scheme.foreground, ...ANSI.map((key) => scheme[key])].map(hex);
  const name = typeof scheme.name === "string" ? scheme.name.trim() : "";
  if (!name || colors.includes(null)) {
    skipped.push(file);
    continue;
  }
  const [background, foreground, ...ansi] = colors;
  entries.push([name, background, foreground, hex(scheme.cursorColor) ?? "", ansi.join(" ")]);
}
entries.sort((a, b) => a[0].localeCompare(b[0], "en", { sensitivity: "base" }));

const commit = execFileSync("git", ["-C", source, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();
const date = execFileSync("git", ["-C", source, "log", "-1", "--format=%cs"], { encoding: "utf8" }).trim();
const notice = `The theme gallery comes from iTerm2-Color-Schemes (https://github.com/mbadolato/iTerm2-Color-Schemes),\ncommit ${commit} (${date}). Each scheme belongs to its author.\n\n${readFileSync(join(source, "LICENSE"), "utf8")}`;
// The license travels inside the data, which is what the app ships.
writeFileSync(join(root, "src/assets/color-schemes.json"), `${JSON.stringify({ source: `mbadolato/iTerm2-Color-Schemes@${commit}`, date, license: notice, schemes: entries })}\n`);
writeFileSync(join(root, "src/assets/LICENSE-color-schemes.txt"), notice);
console.log(`${entries.length} schemes written, ${skipped.length} skipped${skipped.length ? `: ${skipped.join(", ")}` : ""}`);

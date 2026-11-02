// Builds the Nebula interpreter (crates/nebula-sh) and stages it where Tauri expects
// sidecar binaries: src-tauri/binaries/nebula-sh-<target-triple>[.exe].
//
//   node scripts/build-sidecar.mjs            debug build
//   node scripts/build-sidecar.mjs --release  release build
//   node scripts/build-sidecar.mjs --release --target aarch64-pc-windows-msvc
//
// `tauri build --target …` runs this with TAURI_ENV_TARGET_TRIPLE set, so the sidecar
// is built for the same machine as the app.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const release = process.argv.includes("--release");
const run = (command, args) => execFileSync(command, args, { cwd: root, stdio: ["ignore", "pipe", "inherit"], encoding: "utf8" });

const host = /host: (\S+)/.exec(run("rustc", ["-vV"]))?.[1];
if (!host) throw new Error("Could not read the Rust host target from `rustc -vV`.");
const option = process.argv.indexOf("--target");
const triple = (option >= 0 ? process.argv[option + 1] : undefined) ?? process.env.TAURI_ENV_TARGET_TRIPLE ?? host;
// The host build stays in target/<profile>, where the release workflow looks for it.
const cross = triple !== host;

execFileSync("cargo", ["build", "--locked", "-p", "nebula-sh", ...(release ? ["--release"] : []), ...(cross ? ["--target", triple] : [])], { cwd: root, stdio: "inherit" });

const extension = triple.includes("windows") ? ".exe" : "";
const source = join(root, "target", ...(cross ? [triple] : []), release ? "release" : "debug", `nebula-sh${extension}`);
const target = join(root, "src-tauri", "binaries", `nebula-sh-${triple}${extension}`);
mkdirSync(dirname(target), { recursive: true });
copyFileSync(source, target);
console.log(`Staged ${target}`);

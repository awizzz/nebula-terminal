// Builds the Nebula interpreter (crates/nebula-sh) and stages it where Tauri expects
// sidecar binaries: src-tauri/binaries/nebula-sh-<target-triple>[.exe].
//
//   node scripts/build-sidecar.mjs            debug build
//   node scripts/build-sidecar.mjs --release  release build
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const release = process.argv.includes("--release");
const run = (command, args) => execFileSync(command, args, { cwd: root, stdio: ["ignore", "pipe", "inherit"], encoding: "utf8" });

const triple = /host: (\S+)/.exec(run("rustc", ["-vV"]))?.[1];
if (!triple) throw new Error("Could not read the Rust host target from `rustc -vV`.");

execFileSync("cargo", ["build", "--locked", "-p", "nebula-sh", ...(release ? ["--release"] : [])], { cwd: root, stdio: "inherit" });

const extension = triple.includes("windows") ? ".exe" : "";
const source = join(root, "target", release ? "release" : "debug", `nebula-sh${extension}`);
const target = join(root, "src-tauri", "binaries", `nebula-sh-${triple}${extension}`);
mkdirSync(dirname(target), { recursive: true });
copyFileSync(source, target);
console.log(`Staged ${target}`);

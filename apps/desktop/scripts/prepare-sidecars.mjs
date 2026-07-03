import { copyFileSync, mkdirSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import { execFileSync } from "node:child_process";

const desktopDir = resolve(import.meta.dirname, "..");
const repoRoot = resolve(desktopDir, "../..");
const binariesDir = resolve(desktopDir, "src-tauri/binaries");
const targetTriple = execFileSync("rustc", ["--print", "host-tuple"], {
  cwd: repoRoot,
  encoding: "utf8"
}).trim();
const extension = process.platform === "win32" ? ".exe" : "";

if (!targetTriple) {
  throw new Error("Unable to determine Rust host target triple");
}

execFileSync("cargo", ["build", "--release", "--bin", "blipd", "--bin", "blip"], {
  cwd: repoRoot,
  stdio: "inherit"
});

mkdirSync(binariesDir, { recursive: true });

for (const binary of ["blipd", "blip"]) {
  const source = join(repoRoot, "target/release", `${binary}${extension}`);
  const destination = join(binariesDir, `${binary}-${targetTriple}${extension}`);
  copyFileSync(source, destination);
  console.log(`prepared ${basename(destination)}`);
}

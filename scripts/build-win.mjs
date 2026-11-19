/**
 * Script de compilation portable Windows pour PodDesktop.
 * - Compile le frontend Vite minimal (dist-web/)
 * - Compile le binaire Rust MSVC optimisé (opt-level="z", LTO, strip, WebView2 statique)
 * - Copie l'exécutable autonome vers dist/PodDesktop.exe
 * - Vérifie la contrainte de taille cible (< 12 Mo)
 */
import { execSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const distDir = join(root, "dist");
const targetExe = join(root, "src-tauri", "target", "release", "poddesktop.exe");
const outExe = join(distDir, "PodDesktop.exe");
const cargoBinDir = join(homedir(), ".cargo", "bin");
const env = {
  ...process.env,
  PATH: existsSync(cargoBinDir) ? `${cargoBinDir};${process.env.PATH || ""}` : process.env.PATH,
};

console.log("[build:win] 1/3 — Compilation du frontend (Vite)...");
execSync("npm run build:web", { cwd: root, stdio: "inherit", env });

console.log("[build:win] 2/3 — Compilation du binaire natif Rust (MSVC, release + assets embarqués)...");
execSync("cargo build --release --features custom-protocol --manifest-path src-tauri/Cargo.toml", {
  cwd: root,
  stdio: "inherit",
  env,
});

if (!existsSync(targetExe)) {
  throw new Error(`Exécutable introuvable après compilation : ${targetExe}`);
}

console.log("[build:win] 3/3 — Assemblage du package portable dans dist/...");
mkdirSync(distDir, { recursive: true });
copyFileSync(targetExe, outExe);

const sizeBytes = statSync(outExe).size;
const sizeMb = (sizeBytes / (1024 * 1024)).toFixed(2);
console.log(`[build:win] Terminé : ${outExe} (${sizeMb} Mo)`);
if (sizeBytes > 12 * 1024 * 1024) {
  console.warn(`[build:win] ATTENTION : Le binaire dépasse la cible de 12 Mo (${sizeMb} Mo).`);
} else {
  console.log(`[build:win] OK : Poids conforme à l'objectif (< 12 Mo).`);
}

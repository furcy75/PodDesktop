/**
 * Script de compilation portable Linux pour PodDesktop.
 * - Compile le frontend Vite minimal (dist-web/)
 * - Compile le binaire ELF optimisé (dist/PodDesktop)
 * - Construit également l'AppImage portable (dist/PodDesktop.AppImage) si disponible
 */
import { execSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, statSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const distDir = join(root, "dist");
const targetBin = join(root, "src-tauri", "target", "release", "poddesktop");
const outBin = join(distDir, "PodDesktop");

console.log("[build:linux] 1/3 — Compilation du frontend (Vite)...");
execSync("npm run build:web", { cwd: root, stdio: "inherit" });

console.log("[build:linux] 2/3 — Compilation Tauri (ELF + AppImage)...");
execSync("npx tauri build --bundles appimage", { cwd: root, stdio: "inherit" });

mkdirSync(distDir, { recursive: true });

if (existsSync(targetBin)) {
  copyFileSync(targetBin, outBin);
  const sizeMb = (statSync(outBin).size / (1024 * 1024)).toFixed(2);
  console.log(`[build:linux] Binaire ELF portable : ${outBin} (${sizeMb} Mo)`);
}

const appImageDir = join(root, "src-tauri", "target", "release", "bundle", "appimage");
if (existsSync(appImageDir)) {
  const appImages = readdirSync(appImageDir).filter((f) => f.endsWith(".AppImage"));
  if (appImages.length > 0) {
    const srcAppImage = join(appImageDir, appImages[0]);
    const outAppImage = join(distDir, "PodDesktop.AppImage");
    copyFileSync(srcAppImage, outAppImage);
    const sizeMb = (statSync(outAppImage).size / (1024 * 1024)).toFixed(2);
    console.log(`[build:linux] AppImage portable : ${outAppImage} (${sizeMb} Mo)`);
  }
}

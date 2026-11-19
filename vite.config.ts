import { defineConfig } from "vite";

// Configuration Vite minimale pour un bundle frontend ultra-léger (< 80 Ko)
// compatible avec Tauri v2 en mode dev et production.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    target: ["es2022", "chrome105", "safari15"],
    outDir: "dist-web",
    emptyOutDir: true,
    minify: "esbuild",
    sourcemap: false,
    cssCodeSplit: false,
    rollupOptions: {
      output: {
        manualChunks: undefined,
      },
    },
  },
});

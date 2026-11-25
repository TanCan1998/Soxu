import fs from "node:fs";
import path from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";
import { viteSingleFile } from "vite-plugin-singlefile";

function emitStandalonePreviewPlugin(): Plugin {
  return {
    name: "soxu-emit-standalone-preview",
    closeBundle() {
      const builtPath = path.resolve(__dirname, "dist-preview", "desktop.html");
      const targetPath = path.resolve(__dirname, "../../design-system/soxu/preview.html");
      if (fs.existsSync(builtPath)) {
        fs.mkdirSync(path.dirname(targetPath), { recursive: true });
        fs.copyFileSync(builtPath, targetPath);
      }
    },
  };
}

export default defineConfig({
  plugins: [react(), tailwindcss(), viteSingleFile(), emitStandalonePreviewPlugin()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  build: {
    target: "es2022",
    outDir: "dist-preview",
    emptyOutDir: true,
    rollupOptions: {
      input: path.resolve(__dirname, "desktop.html"),
    },
  },
});

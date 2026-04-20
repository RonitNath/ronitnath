import { readdirSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import solidPlugin from "vite-plugin-solid";
import { vanillaExtractPlugin } from "@vanilla-extract/vite-plugin";

const rootDir = fileURLToPath(new URL(".", import.meta.url));
const entriesDir = resolve(rootDir, "entries");

const input: Record<string, string> = {};
for (const file of readdirSync(entriesDir)) {
  if (file.endsWith(".ts") || file.endsWith(".tsx")) {
    const key = file.replace(/\.tsx?$/, "");
    input[key] = resolve(entriesDir, file);
  }
}

export default defineConfig({
  plugins: [solidPlugin(), vanillaExtractPlugin()],
  resolve: {
    alias: {
      "@": resolve(rootDir, "src"),
    },
  },
  build: {
    manifest: true,
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      input,
    },
  },
});

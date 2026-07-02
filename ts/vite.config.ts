import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// Askama templates reference build output by literal path, so entries keep
// stable, unhashed names — no runtime manifest lookup needed. New island?
// Add its entry here and one <script type="module"> tag on its page.
export default defineConfig({
  plugins: [solid()],
  build: {
    outDir: "../static/dist",
    emptyOutDir: true,
    rollupOptions: {
      input: {
        site: "src/entries/site.ts",
        guestbook: "src/entries/guestbook.tsx",
      },
      output: {
        entryFileNames: "[name].js",
        chunkFileNames: "chunks/[name]-[hash].js",
        assetFileNames: "assets/[name][extname]",
      },
    },
  },
});

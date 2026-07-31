import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

export default defineConfig({
  base: "/static/dist/",
  plugins: [solid()],
  build: {
    emptyOutDir: true,
    outDir: "static/dist",
    rollupOptions: {
      input: {
        site: "ts/src/entries/site.tsx",
        event_rsvp: "ts/src/entries/event_rsvp.tsx",
        events_admin: "ts/src/entries/events_admin.tsx",
      },
      output: {
        entryFileNames: "[name].js",
        chunkFileNames: "chunks/[name]-[hash].js",
        assetFileNames: "assets/[name]-[hash][extname]",
      },
    },
  },
});

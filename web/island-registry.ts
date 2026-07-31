import type { IslandRegistry } from "@isoastra/web-runtime/islands";

export const islandLoaders = {
  "public-root": () => import("./islands/PublicRoot"),
} satisfies IslandRegistry;

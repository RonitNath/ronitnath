import "@/styles/global.css";
import { hydrateIslands } from "@/islands/hydrate";
import { islands } from "@/islands/registry";

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", () => hydrateIslands(islands));
} else {
  hydrateIslands(islands);
}

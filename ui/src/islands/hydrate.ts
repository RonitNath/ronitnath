import { render } from "solid-js/web";
import type { IslandRegistry } from "./registry";

export function hydrateIslands(registry: IslandRegistry): void {
  const elements = document.querySelectorAll<HTMLElement>("[data-island]");
  for (const el of elements) {
    const name = el.dataset["island"];
    if (!name) continue;
    const Component = registry[name];
    if (!Component) {
      console.warn(`[islands] unknown island: ${name}`);
      continue;
    }
    const raw = el.dataset["bootstrap"];
    const bootstrap: unknown = raw ? JSON.parse(raw) : undefined;
    render(() => Component({ bootstrap }), el);
  }
}

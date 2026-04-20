import type { Component } from "solid-js";
import { ModeToggle } from "./mode-toggle";

export type IslandProps = { bootstrap?: unknown };
export type IslandComponent = Component<IslandProps>;
export type IslandRegistry = Record<string, IslandComponent>;

export const islands: IslandRegistry = {
  "mode-toggle": ModeToggle,
};

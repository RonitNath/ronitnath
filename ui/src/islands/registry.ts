import type { Component } from "solid-js";
import { AdminEventControls } from "./admin-event-controls";
import { AdminEventsList } from "./admin-events-list";
import { ModeToggle } from "./mode-toggle";
import { RsvpForm } from "./rsvp-form";
import { SignupForm } from "./signup-form";

export type IslandProps = { bootstrap?: unknown };
export type IslandComponent = Component<IslandProps>;
export type IslandRegistry = Record<string, IslandComponent>;

export const islands: IslandRegistry = {
  "mode-toggle": ModeToggle,
  "rsvp-form": RsvpForm,
  "signup-form": SignupForm,
  "admin-event-controls": AdminEventControls,
  "admin-events-list": AdminEventsList,
};

import { onMount } from "solid-js";

import { initErrorBeacon } from "../lib/beacon";
import { initNav } from "../lib/nav";
import { initTheme } from "../lib/theme";

/** Enhances the server-rendered navigation and theme controls without replacing
 * their no-JavaScript HTML fallback. */
export default function SiteChrome() {
  onMount(() => {
    initErrorBeacon();
    initNav();
    initTheme();
  });
  return null;
}

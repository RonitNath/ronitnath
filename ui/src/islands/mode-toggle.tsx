import { createSignal, onMount } from "solid-js";
import * as styles from "@/styles/mode-toggle.css";

type Theme = "dark" | "light";

const COOKIE = "__iso_theme";
const ONE_YEAR = 60 * 60 * 24 * 365;

function readTheme(): Theme {
  const match = document.cookie.match(/__iso_theme=([^;]+)/);
  if (match?.[1] === "light") return "light";
  return (document.documentElement.dataset["theme"] as Theme | undefined) ?? "dark";
}

function writeTheme(theme: Theme): void {
  document.documentElement.dataset["theme"] = theme;
  document.cookie = `${COOKIE}=${theme}; path=/; max-age=${ONE_YEAR}; SameSite=Lax`;
}

export function ModeToggle() {
  const [theme, setTheme] = createSignal<Theme>("dark");

  onMount(() => {
    setTheme(readTheme());
  });

  const toggle = () => {
    const next: Theme = theme() === "dark" ? "light" : "dark";
    writeTheme(next);
    setTheme(next);
  };

  return (
    <button
      type="button"
      class={styles.toggle}
      onClick={toggle}
      aria-label="Toggle light/dark"
      title="Toggle light/dark"
    >
      <span aria-hidden="true">{theme() === "dark" ? "☀" : "☾"}</span>
    </button>
  );
}

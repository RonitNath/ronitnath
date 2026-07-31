import type { Component } from "solid-js";
import { Show, createSignal, onMount } from "solid-js";

type Props = { page?: "home" | "calendar" };
type Theme = "dark" | "light";

const Sun: Component = () => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.93 4.93l1.42 1.42M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.42-1.42M17.66 6.34l1.41-1.41"/></svg>;
const Moon: Component = () => <svg viewBox="0 0 24 24" fill="currentColor"><path d="M20.5 14.2A8.5 8.5 0 0 1 9.8 3.5a8.5 8.5 0 1 0 10.7 10.7Z"/></svg>;

const PublicRoot: Component<Props> = (props) => {
  const [theme, setTheme] = createSignal<Theme>("dark");
  const [menu, setMenu] = createSignal(false);
  onMount(() => {
    const saved = localStorage.getItem("theme");
    const systemLight = typeof window.matchMedia === "function" && window.matchMedia("(prefers-color-scheme: light)").matches;
    const initial: Theme = saved === "light" || saved === "dark" ? saved : systemLight ? "light" : "dark";
    document.documentElement.dataset.theme = initial;
    setTheme(initial);
  });
  const toggleTheme = () => {
    const next: Theme = theme() === "dark" ? "light" : "dark";
    document.documentElement.dataset.theme = next;
    localStorage.setItem("theme", next);
    setTheme(next);
  };
  const page = () => props.page === "calendar" ? "calendar" : "home";
  return <>
    <div class="starfield" aria-hidden="true"><div class="stars-dim"/><div class="stars-med"/><div class="stars-bright"/></div><div class="nebula" aria-hidden="true"/>
    <header class="topbar"><button class="nav-toggle" type="button" aria-label="Open menu" aria-controls="menu" aria-expanded={menu()} onClick={() => setMenu(!menu())}><span class="nav-toggle-bar"/><span class="nav-toggle-bar"/><span class="nav-toggle-bar"/></button><a class="brand" href="/">Ronit Nath</a><div class={`menu${menu() ? " open" : ""}`} id="menu"><nav class="nav"><a class={page() === "home" ? "active" : ""} href="/">Home</a><a class={page() === "calendar" ? "active" : ""} href="/calendar">Calendar</a></nav><div class="auth"><button class="theme-toggle" type="button" aria-label="Toggle color theme" onClick={toggleTheme}><span class="theme-toggle-icon" aria-hidden="true"><Show when={theme() === "dark"} fallback={<Moon/>}><Sun/></Show></span></button></div></div><div class={`drawer-overlay${menu() ? " open" : ""}`} onClick={() => setMenu(false)}/></header>
    <main class="content"><Show when={page() === "home"} fallback={<Calendar/>}><Home/></Show></main>
  </>;
};

const Home: Component = () => <section class="home-hero"><div class="home-card"><h1>Ronit Nath</h1><p class="tagline">Founder of Isoastra</p><ul class="social-links"><li><a href="https://github.com/RonitNath" rel="me noopener" target="_blank">GitHub</a></li><li><a href="https://instagram.com/ronit_nath" rel="me noopener" target="_blank">Instagram</a></li><li><a href="https://linkedin.com/in/ronitn" rel="me noopener" target="_blank">LinkedIn</a></li><li><a href="mailto:ronit@isoastra.com">Email</a></li></ul></div></section>;

const Calendar: Component = () => <><section class="calendar-heading"><div><p class="eyebrow">Calendar</p><h1>Upcoming plans</h1></div><nav class="calendar-nav" aria-label="Month navigation"><a class="button-link" href="/calendar?month=2026-08">Browse month</a></nav></section><section class="calendar-agenda"><h2>Agenda — next 12 months</h2><div class="empty-state"><p>No visible plans ahead.</p><p class="muted">Browse a past month to revisit earlier plans.</p></div></section></>;

export default PublicRoot;

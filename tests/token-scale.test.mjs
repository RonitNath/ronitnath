import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

test("the public heading treatment stays on the shared restrained scale", async () => {
  const [manifest, scales, components, events] = await Promise.all([
    readFile(new URL("../package.json", import.meta.url), "utf8"),
    readFile(
      new URL("../node_modules/@isoastra/tokens/scales.css", import.meta.url),
      "utf8",
    ),
    readFile(new URL("../static/css/components.css", import.meta.url), "utf8"),
    readFile(new URL("../static/css/events.css", import.meta.url), "utf8"),
  ]);

  assert.equal(JSON.parse(manifest).dependencies["@isoastra/tokens"], "0.4.0");
  assert.match(scales, /--text-3xl:[^;]*1\.625rem/);
  assert.match(scales, /--text-display:[^;]*1\.75rem/);
  assert.match(components, /\.home-card h1[\s\S]*font-size: var\(--text-display\)/);
  assert.match(components, /\.hero h1[\s\S]*font-size: var\(--text-3xl\)/);
  assert.match(events, /\.event-hero h1[^}]*var\(--text-display\)/);
});

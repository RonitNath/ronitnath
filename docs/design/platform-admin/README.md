# platform-admin — the design board

Diagrams for `docs/stories/platform-admin-requirements.md`. Built from
`src/board.ts`, which is a pure function: the whole board is emitted as shape
records, so it is diffable and rebuilt from source after every correction
rather than nudged by hand.

```sh
pnpm install
npx tsc -p .            # vite does not typecheck, and tldraw validates at runtime
pnpm build              # dist/board.html (canvas) + dist/index.html (the report)
python3 -m http.server 8771 --directory dist
# → http://127.0.0.1:8771/         the report
# → http://127.0.0.1:8771/board.html   the fourteen frames
```

The build output is not all committed: `dist/index.html` (the report) is, and
`dist/board.html` is not — it is 5.7 MB of inlined assets that `pnpm build`
regenerates in five seconds.

`platform-admin.tldr` is the same records, exported headless
(`bun run export.ts`; it hangs after writing, so kill it). It opens in
tldraw.com or the desktop app and survives after the server is gone.

Reviewed frames are in `shots/` — every one viewed at 1600×1000, both the
1440 and 390 UI mocks among them. Guidance: `~/dev/context/resources/tldraw.md`.
Never publish a board as a Claude artifact.

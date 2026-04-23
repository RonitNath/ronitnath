// Fresh sqlite DB per test run, then exec the built ronitnath binary.
// Expects `cargo build` to have produced target/debug/ronitnath.
import { spawn } from "node:child_process";
import { mkdirSync, rmSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "../../..");
const binary = resolve(repoRoot, "target/debug/ronitnath");
const tmpDir = resolve(repoRoot, "ui/tests/e2e/.tmp");
const dbPath = resolve(tmpDir, "events-e2e.db");

rmSync(dbPath, { force: true });
rmSync(`${dbPath}-wal`, { force: true });
rmSync(`${dbPath}-shm`, { force: true });
mkdirSync(tmpDir, { recursive: true });

const child = spawn(binary, [], {
  cwd: repoRoot,
  stdio: "inherit",
  env: {
    ...process.env,
    DATABASE_URL: `sqlite://${dbPath}`,
  },
});

const shutdown = (signal) => {
  child.kill(signal);
};
process.on("SIGINT", () => shutdown("SIGINT"));
process.on("SIGTERM", () => shutdown("SIGTERM"));

child.on("exit", (code) => {
  process.exit(code ?? 0);
});

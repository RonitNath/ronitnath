import type { ClientErrorReport } from "../generated/ClientErrorReport";

// Client JS errors land in the same server log as everything else — see
// src/handlers/client_errors.rs. Capped so a broken error handler itself
// can't spam the network or the log.
const MAX_REPORTS_PER_PAGE = 10;
let sent = 0;

function send(report: ClientErrorReport): void {
  if (sent >= MAX_REPORTS_PER_PAGE) return;
  sent += 1;

  fetch("/api/client-errors", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(report),
    keepalive: true,
  }).catch(() => {
    // Nothing useful to do if the beacon itself fails to send.
  });
}

export function initErrorBeacon(): void {
  window.addEventListener("error", (event) => {
    send({
      message: event.message,
      source: event.filename ?? "",
      line: event.lineno ?? 0,
      col: event.colno ?? 0,
      stack: event.error instanceof Error ? (event.error.stack ?? "") : "",
    });
  });

  window.addEventListener("unhandledrejection", (event) => {
    const reason = event.reason;
    send({
      message: reason instanceof Error ? reason.message : String(reason),
      source: "unhandledrejection",
      line: 0,
      col: 0,
      stack: reason instanceof Error ? (reason.stack ?? "") : "",
    });
  });
}

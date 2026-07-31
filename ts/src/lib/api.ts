export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function assertShape<T>(
  value: unknown,
  guard: (candidate: unknown) => candidate is T,
  label: string,
): asserts value is T {
  if (!guard(value)) throw new Error(`Invalid ${label} response.`);
}

function hasJsonContentType(res: Response): boolean {
  const contentType = res.headers.get("content-type")?.toLowerCase() ?? "";
  return contentType.includes("application/json") || contentType.includes("+json");
}

export async function jsonBody(res: Response, label: string): Promise<unknown> {
  if (!hasJsonContentType(res)) throw new Error(`${label} returned a non-JSON response.`);
  try {
    return await res.json();
  } catch {
    throw new Error(`${label} returned invalid JSON.`);
  }
}

export async function errorMessage(res: Response, fallback: string): Promise<string> {
  if (!hasJsonContentType(res)) return fallback;
  const body: unknown = await res.json().catch(() => null);
  return isRecord(body) && typeof body.error === "string" ? body.error : fallback;
}

// Read once per call rather than cached: a page never changes this after
// load, but re-reading keeps the helper correct if a future navigation swaps it.
export function csrfToken(): string {
  return document.querySelector('meta[name="csrf-token"]')?.getAttribute("content") ?? "";
}

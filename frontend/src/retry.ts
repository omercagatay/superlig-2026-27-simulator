/* How the site waits out a busy backend. The server answers 503 when its
   simulation queue is full and 429 when one visitor has hit the rate limit,
   both with a Retry-After header. No React and no DOM, so retry.test.mjs
   tests it directly. */

/** A failed API call: the HTTP status and the backend's plain-text body. */
export class ApiError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
  }
}

/** Retries after the first busy answer, so at most four requests in all. */
export const MAX_RETRIES = 3;
/** The longest single wait. A server that asks for more is not asked
 *  again: the rate limit's Retry-After is exact, so asking sooner would
 *  only be turned away. */
export const MAX_WAIT_MS = 10_000;

/** A status that means "busy, ask again shortly". */
export function isBusy(status: number): boolean {
  return status === 503 || status === 429;
}

/** A Retry-After header in milliseconds: delay-seconds ("5") or an HTTP
 *  date. null when missing or unreadable; a date in the past is 0. */
export function parseRetryAfter(value: string | null, now = Date.now()): number | null {
  const v = value?.trim();
  if (!v) return null;
  if (/^\d+$/.test(v)) return Number(v) * 1000;
  // An HTTP date always names its day and month; this keeps Date.parse's
  // leniency from reading "-1" or "1.5" as a year.
  const at = /[a-z]/i.test(v) ? Date.parse(v) : Number.NaN;
  return Number.isFinite(at) ? Math.max(0, at - now) : null;
}

/** The wait before retry `attempt` (0-based): what the server asked for,
 *  else 1 s, 2 s, 4 s, never more than MAX_WAIT_MS. null when the server
 *  asked for longer than that: give up now rather than wait it out. */
export function retryDelay(attempt: number, retryAfterMs: number | null): number | null {
  if (retryAfterMs != null) return retryAfterMs <= MAX_WAIT_MS ? retryAfterMs : null;
  return Math.min(MAX_WAIT_MS, 1000 * 2 ** attempt);
}

function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) return reject(signal.reason);
    const onAbort = () => {
      clearTimeout(timer);
      reject(signal?.reason);
    };
    const timer = setTimeout(() => {
      signal?.removeEventListener("abort", onAbort);
      resolve();
    }, ms);
    signal?.addEventListener("abort", onAbort, { once: true });
  });
}

export interface RetryOptions {
  retries?: number;
  signal?: AbortSignal;
  /** Injected by the tests so they need not wait. */
  wait?: (ms: number, signal?: AbortSignal) => Promise<void>;
}

/** Sends a request, repeating it while the server is busy. Returns the
 *  first successful response; throws ApiError for any other status, or for
 *  a busy one once the retries are spent or the server asks for a longer
 *  wait than MAX_WAIT_MS. */
export async function sendWithRetry(
  send: () => Promise<Response>,
  { retries = MAX_RETRIES, signal, wait = sleep }: RetryOptions = {},
): Promise<Response> {
  for (let attempt = 0; ; attempt++) {
    const resp = await send();
    if (resp.ok) return resp;
    const delay =
      isBusy(resp.status) && attempt < retries
        ? retryDelay(attempt, parseRetryAfter(resp.headers.get("Retry-After")))
        : null;
    if (delay == null) {
      const body = (await resp.text()).trim();
      throw new ApiError(resp.status, body || `HTTP ${resp.status}`);
    }
    // Drain the busy answer so the connection can be reused.
    await resp.text().catch(() => "");
    await wait(delay, signal);
  }
}

/* How the site waits out a busy backend. The server answers 503 when its
   simulation queue is full and 429 when one visitor has hit the rate limit,
   both with a Retry-After header. No React and no DOM, so retry.test.mjs
   tests it directly. */

/** A failed API call: the HTTP status, the backend's plain-text body, and
 *  the Retry-After it sent in whole seconds (null when it sent none). */
export class ApiError extends Error {
  status: number;
  retryAfter: number | null;

  constructor(status: number, message: string, retryAfter: number | null = null) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.retryAfter = retryAfter;
  }
}

/** Retries after the first busy answer, so at most four requests in all. */
export const MAX_RETRIES = 3;
/** The longest single wait. Equal to the server's BUSY_RETRY_AFTER_SECS
 *  (src/handlers.rs), so the 503 it sends is waited out in full; change
 *  the two together. A 429 that asks for longer is not asked again: the
 *  rate limit's Retry-After is exact, so asking sooner would only be
 *  turned away. */
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

/** The wait before retry `attempt` (0-based) of a busy answer: without a
 *  Retry-After, 1 s, 2 s, 4 s. A 503's Retry-After is the server's guess
 *  at when its queue drains, a hint: wait for it, never more than
 *  MAX_WAIT_MS. A 429's is exact: wait for it when it is within
 *  MAX_WAIT_MS, else null, giving up now rather than waiting it out. */
export function retryDelay(status: number, attempt: number, retryAfterMs: number | null): number | null {
  if (retryAfterMs == null) return Math.min(MAX_WAIT_MS, 1000 * 2 ** attempt);
  if (status === 503) return Math.min(MAX_WAIT_MS, retryAfterMs);
  return retryAfterMs <= MAX_WAIT_MS ? retryAfterMs : null;
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
 *  a busy one once the retries are spent or a 429 asks for a longer wait
 *  than MAX_WAIT_MS. */
export async function sendWithRetry(
  send: () => Promise<Response>,
  { retries = MAX_RETRIES, signal, wait = sleep }: RetryOptions = {},
): Promise<Response> {
  for (let attempt = 0; ; attempt++) {
    const resp = await send();
    if (resp.ok) return resp;
    const retryAfterMs = parseRetryAfter(resp.headers.get("Retry-After"));
    const delay =
      isBusy(resp.status) && attempt < retries ? retryDelay(resp.status, attempt, retryAfterMs) : null;
    if (delay == null) {
      const body = (await resp.text()).trim();
      const retryAfter = retryAfterMs == null ? null : Math.ceil(retryAfterMs / 1000);
      throw new ApiError(resp.status, body || `HTTP ${resp.status}`, retryAfter);
    }
    // Drain the busy answer so the connection can be reused.
    await resp.text().catch(() => "");
    await wait(delay, signal);
  }
}

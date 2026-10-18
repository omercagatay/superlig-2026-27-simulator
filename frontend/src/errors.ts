/* What the error line says when an API call fails. The message is always
   localized; the backend's own words, English only, ride along as a
   secondary detail where they help. No React, so errors.test.mjs tests it
   directly. */

import type { Key } from "./i18n";
// With the extension: node's test runner loads this file as it is.
import { ApiError } from "./retry.ts";

/** The call that failed. A 502 from the refresh means TFF did not answer;
 *  anywhere else it comes from a proxy in front of the server. */
export type Operation = "forecast" | "refresh";

export interface Failure {
  error: unknown;
  during: Operation;
}

export interface ErrorText {
  message: string;
  /** The backend's message, shown beside it; null when it adds nothing. */
  detail: string | null;
}

export function describeError(e: unknown, during: Operation, t: (key: Key) => string): ErrorText {
  if (e instanceof ApiError) {
    if (e.status === 503) return { message: t("serverBusy"), detail: null };
    if (e.status === 429) {
      const message =
        e.retryAfter != null && e.retryAfter > 0
          ? t("tooManyRequests").replace("{n}", String(e.retryAfter))
          : t("tooManyRequestsSoon");
      return { message, detail: null };
    }
    if (e.status === 400 || e.status === 422) return { message: t("settingsRejected"), detail: e.message };
    if (e.status === 502 && during === "refresh") return { message: t("tffUnreachable"), detail: e.message };
    return { message: t("requestFailed"), detail: e.message };
  }
  // fetch rejects with a TypeError when no answer arrives at all: offline,
  // a failed DNS lookup, a dropped connection. Its text is the browser's.
  if (e instanceof TypeError) return { message: t("connectionFailed"), detail: null };
  return { message: t("requestFailed"), detail: e instanceof Error ? e.message : String(e) };
}

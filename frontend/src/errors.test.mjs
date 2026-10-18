// npm test  (node --experimental-strip-types --test src/errors.test.mjs)
import { test } from "node:test";
import assert from "node:assert/strict";
import { describeError } from "./errors.ts";
import { ApiError } from "./retry.ts";

// Stands in for useT(): the messages describeError may pick, in English.
const EN = {
  serverBusy: "The server is busy. Please try again in a moment.",
  tooManyRequests: "Too many requests — try again in {n} s.",
  tooManyRequestsSoon: "Too many requests — try again in a moment.",
  settingsRejected: "These settings were not accepted.",
  tffUnreachable: "Could not reach TFF for live data.",
  connectionFailed: "Connection failed. Please try again.",
  requestFailed: "The request failed.",
};
const t = (key) => {
  assert.ok(key in EN, `unexpected key ${key}`);
  return EN[key];
};

test("a busy queue gets the friendly line, without the backend's text", () => {
  assert.deepEqual(describeError(new ApiError(503, "Simulation queue is full", 10), "forecast", t), {
    message: EN.serverBusy,
    detail: null,
  });
});

test("a rate limit says how long to wait when the server said", () => {
  assert.deepEqual(describeError(new ApiError(429, "Rate limit exceeded", 45), "forecast", t), {
    message: "Too many requests — try again in 45 s.",
    detail: null,
  });
  for (const retryAfter of [null, 0]) {
    assert.equal(describeError(new ApiError(429, "Rate limit exceeded", retryAfter), "refresh", t).message, EN.tooManyRequestsSoon);
  }
});

test("refused settings say so, with the backend's reason as the detail", () => {
  for (const status of [400, 422]) {
    assert.deepEqual(describeError(new ApiError(status, "n_sims must be at most 200000"), "forecast", t), {
      message: EN.settingsRejected,
      detail: "n_sims must be at most 200000",
    });
  }
});

test("a 502 is TFF only on the refresh", () => {
  const e = new ApiError(502, "Scrape error: timed out");
  assert.deepEqual(describeError(e, "refresh", t), { message: EN.tffUnreachable, detail: "Scrape error: timed out" });
  assert.deepEqual(describeError(e, "forecast", t), { message: EN.requestFailed, detail: "Scrape error: timed out" });
});

test("no answer at all is a connection failure", () => {
  assert.deepEqual(describeError(new TypeError("Failed to fetch"), "forecast", t), {
    message: EN.connectionFailed,
    detail: null,
  });
});

test("anything else gets a localized line and keeps its text as the detail", () => {
  assert.deepEqual(describeError(new ApiError(500, "HTTP 500"), "forecast", t), {
    message: EN.requestFailed,
    detail: "HTTP 500",
  });
  assert.deepEqual(describeError(new SyntaxError("Unexpected token <"), "refresh", t), {
    message: EN.requestFailed,
    detail: "Unexpected token <",
  });
  assert.deepEqual(describeError("odd", "forecast", t), { message: EN.requestFailed, detail: "odd" });
});

// npm test  (node --experimental-strip-types --test src/retry.test.mjs)
import { test } from "node:test";
import assert from "node:assert/strict";
import { ApiError, MAX_WAIT_MS, isBusy, parseRetryAfter, retryDelay, sendWithRetry } from "./retry.ts";

test("Retry-After reads as seconds or an HTTP date, anything else as missing", () => {
  const now = Date.parse("2026-10-18T12:00:00Z");
  assert.equal(parseRetryAfter("5", now), 5000);
  assert.equal(parseRetryAfter(" 2 ", now), 2000);
  assert.equal(parseRetryAfter("0", now), 0);
  assert.equal(parseRetryAfter("Sun, 18 Oct 2026 12:00:03 GMT", now), 3000);
  assert.equal(parseRetryAfter("Sun, 18 Oct 2026 11:00:00 GMT", now), 0);
  assert.equal(parseRetryAfter(null, now), null);
  assert.equal(parseRetryAfter("", now), null);
  assert.equal(parseRetryAfter("soon", now), null);
  assert.equal(parseRetryAfter("-1", now), null);
  assert.equal(parseRetryAfter("1.5", now), null);
});

test("waits follow Retry-After, else back off 1, 2, 4 s, and never pass the cap", () => {
  assert.deepEqual([0, 1, 2].map((a) => retryDelay(a, null)), [1000, 2000, 4000]);
  assert.equal(retryDelay(0, 3000), 3000);
  assert.equal(retryDelay(2, 0), 0);
  assert.equal(retryDelay(0, 60_000), MAX_WAIT_MS);
  assert.equal(retryDelay(6, null), MAX_WAIT_MS);
  assert.ok(isBusy(503) && isBusy(429));
  assert.ok(!isBusy(500) && !isBusy(404) && !isBusy(400));
});

/** A fake server that answers with each response in turn. */
function server(...responses) {
  const sent = [];
  const waits = [];
  return {
    sent,
    waits,
    send: async () => {
      sent.push(Date.now());
      return responses[Math.min(sent.length - 1, responses.length - 1)]();
    },
    wait: async (ms) => {
      waits.push(ms);
    },
  };
}

const busy = (status, retryAfter) => () =>
  new Response("busy", { status, headers: retryAfter == null ? {} : { "Retry-After": retryAfter } });
const ok = () => new Response('{"n_sims":2000}', { status: 200 });

test("a busy server is asked again until it answers", async () => {
  const s = server(busy(503, "2"), busy(429), ok);
  const resp = await sendWithRetry(s.send, { wait: s.wait });
  assert.deepEqual(await resp.json(), { n_sims: 2000 });
  assert.equal(s.sent.length, 3);
  assert.deepEqual(s.waits, [2000, 2000]);
});

test("a server that stays busy fails with its status after three retries", async () => {
  const s = server(busy(503, "1"));
  await assert.rejects(sendWithRetry(s.send, { wait: s.wait }), (e) => {
    assert.ok(e instanceof ApiError);
    assert.equal(e.status, 503);
    assert.equal(e.message, "busy");
    return true;
  });
  assert.equal(s.sent.length, 4);
  assert.deepEqual(s.waits, [1000, 1000, 1000]);
});

test("any other failure is not retried and keeps the backend's message", async () => {
  const s = server(() => new Response("n_sims must be between 100 and 200000\n", { status: 400 }));
  await assert.rejects(sendWithRetry(s.send, { wait: s.wait }), {
    name: "ApiError",
    status: 400,
    message: "n_sims must be between 100 and 200000",
  });
  assert.equal(s.sent.length, 1);
  assert.deepEqual(s.waits, []);

  const empty = server(() => new Response("", { status: 500 }));
  await assert.rejects(sendWithRetry(empty.send, { wait: empty.wait }), { status: 500, message: "HTTP 500" });
});

test("aborting cuts a wait short", async () => {
  const abort = new AbortController();
  const s = server(busy(503, "10"));
  const started = Date.now();
  setTimeout(() => abort.abort(), 20);
  await assert.rejects(sendWithRetry(s.send, { signal: abort.signal }), { name: "AbortError" });
  assert.ok(Date.now() - started < 5000);
  assert.equal(s.sent.length, 1);
});

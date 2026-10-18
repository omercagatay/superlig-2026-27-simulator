// npm test  (node --experimental-strip-types --test src/history.test.mjs)
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  byMatchday,
  fill,
  matchOrder,
  roundTicks,
  runningSeries,
  tickDigits,
  valueTicks,
} from "./history.ts";

const scored = (round, date, home, away, hit, logLoss, baseline) => ({
  round,
  date,
  home,
  away,
  home_score: 1,
  away_score: 0,
  outcome_pct: 50,
  outcome: "1",
  hit,
  home_pct: 50,
  draw_pct: 25,
  away_pct: 25,
  logged_at: "2026-08-01",
  log_loss: logLoss,
  baseline_log_loss: baseline,
});

// As the backend sends them: newest matchday first, otherwise in log order.
const matches = [
  scored(3, "2026-08-31", "Samsunspor", "Eyüpspor", true, 0.5, 0.79),
  scored(3, "2026-08-30", "Konyaspor", "Rizespor", false, 1.5, 1.36),
  scored(3, "2026-08-30", "Çorum", "Göztepe", true, 0.7, 0.79),
  scored(3, "2026-08-30", "Beşiktaş", "Kasımpaşa", true, 0.4, 0.79),
  scored(1, "2026-08-16", "Galatasaray", "Gaziantep", false, 2.0, 1.24),
  scored(2, "2026-08-23", "Fenerbahçe", "Alanyaspor", true, 0.6, 0.79),
  scored(2, "2026-08-22", "Şanlıurfaspor", "Trabzonspor", false, 1.2, 1.36),
];

test("matchdays come latest first, matches by date then home club", () => {
  const days = byMatchday(matches);
  assert.deepEqual(
    days.map((d) => d.round),
    [3, 2, 1],
  );
  // Turkish order: Beşiktaş, Çorum, Konyaspor on the same day, then a day later.
  assert.deepEqual(
    days[0].matches.map((m) => m.home),
    ["Beşiktaş", "Çorum", "Konyaspor", "Samsunspor"],
  );
  assert.deepEqual(
    days[1].matches.map((m) => m.home),
    ["Şanlıurfaspor", "Fenerbahçe"],
  );
  // The input is left as it was.
  assert.equal(matches[0].home, "Samsunspor");
  assert.deepEqual(byMatchday([]), []);
});

test("each matchday sums up its own matches", () => {
  const [md3, md2, md1] = byMatchday(matches);
  assert.equal(md3.n, 4);
  assert.equal(md3.hits, 3);
  assert.ok(Math.abs(md3.logLoss - (0.5 + 1.5 + 0.7 + 0.4) / 4) < 1e-12);
  assert.ok(Math.abs(md3.baselineLogLoss - (0.79 * 3 + 1.36) / 4) < 1e-12);
  assert.equal(md2.hits, 1);
  assert.equal(md1.n, 1);
  assert.equal(md1.hits, 0);
  assert.equal(md1.logLoss, 2.0);
});

test("the running series is the season-to-date mean, earliest matchday first", () => {
  const series = runningSeries(matches);
  assert.deepEqual(
    series.map((p) => [p.round, p.n]),
    [
      [1, 1],
      [2, 3],
      [3, 7],
    ],
  );
  assert.equal(series[0].logLoss, 2.0);
  assert.equal(series[0].baselineLogLoss, 1.24);
  assert.ok(Math.abs(series[1].logLoss - (2.0 + 0.6 + 1.2) / 3) < 1e-12);
  // The last point is the headline figure over every scored match.
  const all = matches.reduce((s, m) => s + m.log_loss, 0) / matches.length;
  const allBase = matches.reduce((s, m) => s + m.baseline_log_loss, 0) / matches.length;
  assert.ok(Math.abs(series[2].logLoss - all) < 1e-12);
  assert.ok(Math.abs(series[2].baselineLogLoss - allBase) < 1e-12);
  assert.deepEqual(runningSeries([]), []);
});

test("matchOrder sorts by date before home club", () => {
  const a = scored(1, "2026-08-16", "Zonguldak", "X", true, 1, 1);
  const b = scored(1, "2026-08-17", "Alanyaspor", "X", true, 1, 1);
  assert.ok(matchOrder(a, b) < 0);
  assert.ok(matchOrder(b, a) > 0);
});

test("templates fill their placeholders in either word order", () => {
  assert.equal(fill("Called {hits} of {n}", { hits: "5", n: "9" }), "Called 5 of 9");
  assert.equal(fill("{n} maçta {hits} doğru", { hits: "5", n: "9" }), "9 maçta 5 doğru");
  assert.equal(fill("{a} and {missing}", { a: "x" }), "x and {missing}");
});

test("value ticks are round and cover every value", () => {
  assert.deepEqual(valueTicks([0.95, 1.12]), [0.95, 1, 1.05, 1.1, 1.15]);
  assert.deepEqual(valueTicks([0.98, 1.01, 1.04]), [0.98, 1, 1.02, 1.04]);
  // A flat series still gets an axis around its value.
  assert.deepEqual(valueTicks([1, 1]), [0.95, 1, 1.05]);
  assert.deepEqual(valueTicks([0.62, 2.31]), [0.5, 1, 1.5, 2, 2.5]);
  assert.deepEqual(valueTicks([]), []);
  assert.equal(tickDigits([0.95, 1, 1.05]), 2);
  assert.equal(tickDigits([0.5, 1, 1.5]), 1);
  assert.equal(tickDigits([0, 1, 2]), 0);
  assert.equal(tickDigits([0.98, 1, 1.02]), 2);
  for (const values of [
    [0.7, 1.9],
    [1.001, 1.003],
    [0.3, 0.31],
  ]) {
    const ticks = valueTicks(values);
    assert.ok(ticks[0] <= Math.min(...values) && ticks.at(-1) >= Math.max(...values), `${values}`);
    assert.ok(ticks.length >= 2 && ticks.length <= 6, `${values}: ${ticks}`);
  }
});

test("matchday ticks thin out to fit the width", () => {
  assert.deepEqual(roundTicks(1, 7, 400), [1, 2, 3, 4, 5, 6, 7]);
  assert.deepEqual(roundTicks(1, 7, 100), [2, 4, 6]);
  assert.deepEqual(roundTicks(1, 34, 300), [5, 10, 15, 20, 25, 30]);
  assert.deepEqual(roundTicks(4, 4, 300), [4]);
  // Fewer rounds than one step: label both ends.
  assert.deepEqual(roundTicks(6, 8, 20), [6, 8]);
});

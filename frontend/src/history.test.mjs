// npm test  (node --experimental-strip-types --test src/history.test.mjs)
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  byMatchday,
  fill,
  intervalNote,
  matchOrder,
  modelName,
  modelRows,
  readoutLeft,
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
  model_version: "2026-08",
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

test("the readout sits beside the crosshair and inside the plot", () => {
  // Room on the right: 12px right of the crosshair.
  assert.equal(readoutLeft(50, 150, 400), 62);
  // No room on the right: 12px left of it.
  assert.equal(readoutLeft(300, 150, 400), 138);
  // A 320px phone, crosshair near the middle, readout wider than either
  // side: pinned to whichever edge it would cross, never past it.
  assert.equal(readoutLeft(152, 163, 288), 0);
  assert.equal(readoutLeft(140, 163, 288), 0);
  for (let at = 0; at <= 288; at += 4) {
    const left = readoutLeft(at, 163, 288);
    assert.ok(left >= 0 && left + 163 <= 288, `at ${at}: ${left}`);
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

const EN_WORDS = { current: "Current model", ofMonth: "{month} model", unlabelled: "Unlabelled model" };
const TR_WORDS = { current: "güncel model", ofMonth: "{month} modeli", unlabelled: "etiketsiz model" };

test("models are named for the month they shipped, the running one as current", () => {
  const labels = ["2026-10b", "2026-10a", "2026-08"];
  assert.equal(modelName("2026-10b", "2026-10b", labels, "en", EN_WORDS), "Current model");
  assert.equal(modelName("2026-08", "2026-10b", labels, "en", EN_WORDS), "August model");
  assert.equal(modelName("2026-08", "2026-10b", labels, "tr", TR_WORDS), "Ağustos modeli");
  // The other October model is the current one, so no letter is needed.
  assert.equal(modelName("2026-10a", "2026-10b", labels, "en", EN_WORDS), "October model");
  assert.equal(modelName("2026-10a", "2026-10b", labels, "tr", TR_WORDS), "Ekim modeli");
  assert.equal(modelName("2026-10b", "2026-10b", labels, "tr", TR_WORDS), "güncel model");
  // Once both October models are past, their letters tell them apart.
  const later = ["2026-11", "2026-10b", "2026-10a", "2026-08"];
  assert.equal(modelName("2026-10a", "2026-11", later, "en", EN_WORDS), "October model (a)");
  assert.equal(modelName("2026-10b", "2026-11", later, "en", EN_WORDS), "October model (b)");
  assert.equal(modelName("2026-08", "2026-11", later, "en", EN_WORDS), "August model");
  assert.equal(modelName(null, "2026-10b", labels, "en", EN_WORDS), "Unlabelled model");
  assert.equal(modelName("custom", "2026-10b", labels, "en", EN_WORDS), "custom");
});

const EN_INTERVAL = { interval: "95% CI {lo} to {hi}", matches: "{n} matches", oneMatch: "1 match" };
const TR_INTERVAL = { interval: "%95 GA {lo} ile {hi}", matches: "{n} maç", oneMatch: "1 maç" };

test("the interval note gives the interval and the match count", () => {
  const v = { n: 63, mean: -0.078, std_error: 0.0337, ci95: [-0.144, -0.012] };
  assert.equal(intervalNote(v, "en", EN_INTERVAL), "95% CI −0.144 to −0.012 · 63 matches");
  assert.equal(intervalNote(v, "tr", TR_INTERVAL), "%95 GA −0,144 ile −0,012 · 63 maç");
  // An interval across zero keeps both signs.
  const across = { n: 1200, mean: 0.01, std_error: 0.02, ci95: [-0.0292, 0.0492] };
  assert.equal(intervalNote(across, "en", EN_INTERVAL), "95% CI −0.029 to +0.049 · 1,200 matches");
  assert.equal(intervalNote(across, "tr", TR_INTERVAL), "%95 GA −0,029 ile +0,049 · 1.200 maç");
  // One match has no interval.
  const one = { n: 1, mean: 0.2, std_error: null, ci95: null };
  assert.equal(intervalNote(one, "en", EN_INTERVAL), "1 match");
  assert.equal(intervalNote(one, "tr", TR_INTERVAL), "1 maç");
});

test("the breakdown puts the running model first, scored or not", () => {
  const score = (model_version, scored) => ({
    model_version,
    scored,
    hit_rate_pct: 50,
    log_loss: 1,
    baseline_log_loss: 1.05,
    vs_baseline: { n: scored, mean: -0.05, std_error: 0.1, ci95: [-0.25, 0.15] },
  });
  // Nothing of the running model's scored yet: it still leads, empty.
  const early = modelRows({
    current_model_version: "2026-10b",
    by_model: [score("2026-10a", 9), score("2026-08", 55), score(null, 2)],
  });
  assert.deepEqual(
    early.map((r) => [r.version, r.score?.scored ?? null]),
    [
      ["2026-10b", null],
      ["2026-10a", 9],
      ["2026-08", 55],
      [null, 2],
    ],
  );
  const later = modelRows({
    current_model_version: "2026-10b",
    by_model: [score("2026-10b", 18), score("2026-08", 55)],
  });
  assert.deepEqual(
    later.map((r) => [r.version, r.score?.scored ?? null]),
    [
      ["2026-10b", 18],
      ["2026-08", 55],
    ],
  );
});

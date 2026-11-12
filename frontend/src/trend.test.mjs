// npm test  (node --experimental-strip-types --test src/trend.test.mjs)
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  clubSeries,
  drawable,
  isDefaultForecast,
  MIN_SPAN,
  nowSource,
  ownScale,
  reconstruction,
  reconstructionNote,
  sharedScale,
  sparkLabel,
  sparkPath,
  sparkPoints,
  startCaption,
  trendView,
} from "./trend.ts";

const DEFAULTS = { nSims: 50000, seed: 12345 };

const club = (team, title, rel = 0, topFour = 50, pts = 50) => ({
  team,
  title_pct: title,
  top_two_pct: 0,
  top_four_pct: topFour,
  relegation_pct: rel,
  exp_points: pts,
  mean_position: 9,
});

const snap = (played, latest, clubs, reconstructed = true) => ({
  played,
  latest_result: latest,
  logged_at: "2026-11-12",
  model_version: "2026-10b",
  ...(reconstructed ? { reconstructed: true } : {}),
  n_sims: 50000,
  seed: 12345,
  clubs,
});

const SNAPSHOTS = [
  snap(0, null, [club("Galatasaray", 58.3, 0, 98.9, 76.6), club("Eyüpspor", 0, 19.7, 2.2, 39.9)]),
  snap(9, "2026-08-17", [club("Galatasaray", 59.0, 0, 97.7, 73.6), club("Eyüpspor", 0, 25, 1.3, 38)]),
  snap(18, "2026-08-24", [club("Galatasaray", 84.4, 0, 99.8, 79.7)]),
];
const TREND = { snapshots: SNAPSHOTS };

const team = (name, title) => ({ ...club(name, title, 1, 90, 80), title_odds: null, relegation_odds: null, third_pct: 0, fourth_pct: 0, exp_gd: 0 });

const sim = (over = {}) => ({
  n_sims: 50000,
  seed: 12345,
  teams: [team("Galatasaray", 90)],
  positions: [],
  table: [],
  rivalries: [],
  consensus_champion: "Galatasaray",
  thresholds: [],
  what_if: [],
  elo_overrides: {},
  scenario_applied: null,
  ...over,
});

const asOf = (played, latest = "2026-08-31") => ({ played, fixtures: 306, latest_result: latest });

const WORDS = {
  before: "{v} before the season",
  after: "{v} after {date}",
  now: "{v} now",
  between: "between {lo} and {hi}",
};

test("a club's series follows the snapshots that list it, oldest first", () => {
  const g = clubSeries(SNAPSHOTS, "Galatasaray", null);
  assert.deepEqual(
    g.map((p) => [p.played, p.latest, p.title_pct, p.now, p.reconstructed]),
    [
      [0, null, 58.3, false, true],
      [9, "2026-08-17", 59.0, false, true],
      [18, "2026-08-24", 84.4, false, true],
    ],
  );
  // A snapshot without the club is skipped, not zero-filled.
  assert.equal(clubSeries(SNAPSHOTS, "Eyüpspor", null).length, 2);
  assert.deepEqual(clubSeries(SNAPSHOTS, "Nobody", null), []);
  assert.equal(clubSeries([snap(0, null, [club("A", 1)], false)], "A", null)[0].reconstructed, false);
});

test("the default forecast is the only one that counts as default", () => {
  assert.equal(isDefaultForecast(sim(), DEFAULTS), true);
  assert.equal(isDefaultForecast(sim({ what_if: [{ home: "A", away: "B", outcome: "home" }] }), DEFAULTS), false);
  assert.equal(isDefaultForecast(sim({ elo_overrides: { Galatasaray: 1700 } }), DEFAULTS), false);
  assert.equal(isDefaultForecast(sim({ scenario_applied: "x" }), DEFAULTS), false);
  assert.equal(isDefaultForecast(sim({ n_sims: 1000 }), DEFAULTS), false);
  assert.equal(isDefaultForecast(sim({ seed: 1 }), DEFAULTS), false);
});

test("the on-screen forecast is appended only when default and newer than the log", () => {
  const now = nowSource(TREND, sim(), asOf(27), DEFAULTS);
  assert.deepEqual(now && [now.played, now.latest], [27, "2026-08-31"]);
  const g = clubSeries(SNAPSHOTS, "Galatasaray", now);
  assert.equal(g.length, 4);
  assert.deepEqual([g[3].played, g[3].title_pct, g[3].now, g[3].reconstructed], [27, 90, true, false]);

  // The calendar stands where the last snapshot does: the log alone.
  assert.equal(nowSource(TREND, sim(), asOf(18), DEFAULTS), null);
  assert.equal(nowSource(TREND, sim(), asOf(10), DEFAULTS), null);
  // A what-if, an override, a scenario or a small run never is.
  assert.equal(nowSource(TREND, sim({ what_if: [{ home: "A", away: "B", outcome: "draw" }] }), asOf(27), DEFAULTS), null);
  assert.equal(nowSource(TREND, sim({ elo_overrides: { Galatasaray: 1800 } }), asOf(27), DEFAULTS), null);
  assert.equal(nowSource(TREND, sim({ scenario_applied: "injury" }), asOf(27), DEFAULTS), null);
  assert.equal(nowSource(TREND, sim({ n_sims: 2000 }), asOf(27), DEFAULTS), null);
  // Anything missing.
  assert.equal(nowSource(null, sim(), asOf(27), DEFAULTS), null);
  assert.equal(nowSource({ snapshots: [] }, sim(), asOf(27), DEFAULTS), null);
  assert.equal(nowSource(TREND, null, asOf(27), DEFAULTS), null);
  assert.equal(nowSource(TREND, sim(), null, DEFAULTS), null);
  // A club the forecast does not list gets no now point; nor a club the log lacks.
  assert.equal(clubSeries(SNAPSHOTS, "Eyüpspor", now).length, 2);
  assert.deepEqual(clubSeries(SNAPSHOTS, "Nobody", { ...now, teams: [team("Nobody", 1)] }), []);
});

test("trendView: null without snapshots; `current` only for the default forecast", () => {
  assert.equal(trendView(null, sim(), asOf(18), DEFAULTS), null);
  assert.equal(trendView({ snapshots: [] }, sim(), asOf(18), DEFAULTS), null);
  const v = trendView(TREND, sim(), asOf(18), DEFAULTS);
  assert.equal(v.current, 18);
  assert.equal(v.now, null);
  assert.equal(trendView(TREND, sim({ seed: 2 }), asOf(18), DEFAULTS).current, null);
  assert.equal(trendView(TREND, null, asOf(18), DEFAULTS).current, null);
  assert.equal(trendView(TREND, sim(), asOf(27), DEFAULTS).now.played, 27);
});

test("a series is drawable with two points and something above the floor", () => {
  const g = clubSeries(SNAPSHOTS, "Galatasaray", null);
  assert.equal(drawable(g, "title_pct"), true);
  assert.equal(drawable(g.slice(0, 1), "title_pct"), false);
  assert.equal(drawable(g, "relegation_pct"), false);
  const trace = clubSeries([snap(0, null, [club("A", 0.04)]), snap(9, "2026-08-17", [club("A", 0)])], "A", null);
  assert.equal(drawable(trace, "title_pct"), true);
  assert.equal(drawable(trace, "title_pct", 0.05), false);
});

test("a list shares one scale from 0 to its highest value over the season", () => {
  const g = clubSeries(SNAPSHOTS, "Galatasaray", null);
  const e = clubSeries(SNAPSHOTS, "Eyüpspor", null);
  assert.deepEqual(sharedScale([g, e], "title_pct"), [0, 84.4]);
  assert.deepEqual(sharedScale([g, e], "relegation_pct"), [0, 25]);
  // Never an empty range.
  assert.deepEqual(sharedScale([], "title_pct"), [0, 1]);
});

test("a fact's own scale spans at least its minimum, inside its bounds", () => {
  const series = (values, m = "title_pct") =>
    values.map((v, i) => ({ played: i * 9, latest: null, now: false, reconstructed: false, title_pct: 0, top_four_pct: 0, relegation_pct: 0, exp_points: 0, [m]: v }));
  // A wide range is its own.
  assert.deepEqual(ownScale(series([58.3, 84.4]), "title_pct"), [58.3, 84.4]);
  // Noise near zero spans the minimum from 0, so it draws flat at the bottom.
  assert.deepEqual(ownScale(series([0.1, 0.3]), "title_pct"), [0, MIN_SPAN.title_pct]);
  // Near the top it is pushed down from 100.
  assert.deepEqual(ownScale(series([97.7, 99.8], "top_four_pct"), "top_four_pct"), [90, 100]);
  // In the middle it is centred.
  assert.deepEqual(ownScale(series([40, 42], "relegation_pct"), "relegation_pct"), [36, 46]);
  // Points: a 5-point minimum, no upper bound.
  assert.deepEqual(ownScale(series([76, 77], "exp_points"), "exp_points"), [74, 79]);
  assert.deepEqual(ownScale(series([1, 2], "exp_points"), "exp_points"), [0, 5]);
});

test("points are placed by matches played inside the padding, high values up", () => {
  const s = [
    { played: 0, title_pct: 0 },
    { played: 9, title_pct: 50 },
    { played: 18, title_pct: 100 },
    { played: 21, title_pct: 100 },
  ];
  const pts = sparkPoints(s, "title_pct", [0, 100], 72, 24, 5);
  assert.deepEqual(pts[0], { x: 5, y: 19 });
  assert.equal(pts[1].x, Math.round((5 + (9 / 21) * 62) * 100) / 100);
  assert.equal(pts[1].y, 12);
  assert.deepEqual(pts[3], { x: 67, y: 5 });
  // Values outside the scale are clamped to its edge.
  assert.equal(sparkPoints([{ played: 0, title_pct: 150 }, { played: 9, title_pct: -5 }], "title_pct", [0, 100], 72, 24, 5)[0].y, 5);
  // All at one played count: spread by index instead.
  assert.deepEqual(
    sparkPoints([{ played: 3, title_pct: 0 }, { played: 3, title_pct: 0 }], "title_pct", [0, 1], 20, 10, 0).map((p) => p.x),
    [0, 20],
  );
  assert.deepEqual(sparkPoints([], "title_pct", [0, 1], 20, 10, 0), []);
  assert.equal(sparkPath([{ x: 1, y: 2 }, { x: 3.5, y: 4 }]), "M1 2 L3.5 4");
});

test("where reconstruction ends, and the sentence that says so", () => {
  const words = { until: "Up to {date}.", untilPreseason: "Preseason only.", all: "All of them." };
  const r = (flags, hasNow = false) =>
    reconstruction(
      flags.map((f, i) => snap(i * 9, i === 0 ? null : `2026-08-${10 + i}`, [], f)),
      hasNow,
    );
  assert.deepEqual(r([false, false]), { kind: "none" });
  assert.deepEqual(r([true, true]), { kind: "all" });
  assert.deepEqual(r([true, true, false]), { kind: "until", date: "2026-08-11" });
  assert.deepEqual(r([true, false]), { kind: "until", date: null });
  // A now point after an all-reconstructed log was not computed afterwards.
  assert.deepEqual(r([true, true], true), { kind: "until", date: "2026-08-11" });
  assert.equal(reconstructionNote({ kind: "none" }, "en", words), "");
  assert.equal(reconstructionNote({ kind: "all" }, "en", words), "All of them.");
  assert.equal(reconstructionNote({ kind: "until", date: null }, "en", words), "Preseason only.");
  assert.equal(reconstructionNote({ kind: "until", date: "2026-11-02" }, "en", words), "Up to Mon 2 Nov.");
  assert.equal(reconstructionNote({ kind: "until", date: "2026-11-02" }, "tr", words), "Up to 2 Kas Pzt.");
});

test("the text alternative gives the start, the end and the range", () => {
  const g = clubSeries(SNAPSHOTS, "Galatasaray", null);
  assert.equal(
    sparkLabel(g, "title_pct", "en", WORDS, 18),
    "58.3% before the season, 84.4% now, between 58.3% and 84.4%",
  );
  // Not the default forecast on screen, or the calendar has moved on: dated.
  assert.equal(
    sparkLabel(g, "title_pct", "en", WORDS, null),
    "58.3% before the season, 84.4% after Mon 24 Aug, between 58.3% and 84.4%",
  );
  // A now point reads "now"; Turkish formats.
  const withNow = clubSeries(SNAPSHOTS, "Galatasaray", nowSource(TREND, sim(), asOf(27), DEFAULTS));
  const tr = { before: "sezon öncesi {v}", after: "{date} sonrası {v}", now: "şu an {v}", between: "en düşük {lo}, en yüksek {hi}" };
  assert.equal(
    sparkLabel(withNow, "title_pct", "tr", tr, 27),
    "Sezon öncesi %58,3, şu an %90,0, en düşük %58,3, en yüksek %90,0",
  );
  assert.equal(sparkLabel(withNow.slice(1), "title_pct", "tr", tr, null), "17 Ağu Pzt sonrası %59,0, şu an %90,0, en düşük %59,0, en yüksek %90,0");
  // Zero prints as zero, and a range that prints as one value is left out.
  const e = clubSeries(SNAPSHOTS, "Eyüpspor", null);
  assert.equal(sparkLabel(e, "title_pct", "en", WORDS, null), "0.0% before the season, 0.0% after Mon 17 Aug");
  assert.equal(sparkLabel(g, "exp_points", "en", WORDS, 18), "76.6 before the season, 79.7 now, between 73.6 and 79.7");
  assert.equal(sparkLabel([], "title_pct", "en", WORDS, null), "");
});

test("the caption says where the series started", () => {
  const g = clubSeries(SNAPSHOTS, "Galatasaray", null);
  assert.equal(startCaption(g, "title_pct", "en", WORDS), "58.3% before the season");
  assert.equal(startCaption(g, "exp_points", "tr", { ...WORDS, before: "sezon öncesi {v}" }), "Sezon öncesi 76,6");
  assert.equal(startCaption(g.slice(1), "title_pct", "en", WORDS), "59.0% after Mon 17 Aug");
  assert.equal(startCaption([], "title_pct", "en", WORDS), "");
});

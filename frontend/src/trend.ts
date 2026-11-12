/* Pure helpers behind the season-trend sparklines (Overview's race lists and
   the club dialog), kept apart from React so trend.test.mjs tests them
   directly. The data is GET /api/trend: the default forecast at successive
   states of the league, oldest first. */

import type { AsOf, SimResponse, TeamRow, TrendResponse, TrendSnapshot } from "./api";
// With the extension: node's test runner loads these files as they are.
import { num, pct, shortDate, type Lang } from "./clubs.ts";
import { fill } from "./history.ts";

/** What a sparkline plots. */
export type Measure = "title_pct" | "top_four_pct" | "relegation_pct" | "exp_points";

/** One club's forecast at one state of the league. */
export interface TrendPoint {
  /** Results the forecast stood on. */
  played: number;
  /** Date of the latest of them; null before the first. */
  latest: string | null;
  /** The forecast on screen, appended after the log (see `nowSource`). */
  now: boolean;
  /** Computed after the fact rather than published at the time. */
  reconstructed: boolean;
  title_pct: number;
  top_four_pct: number;
  relegation_pct: number;
  exp_points: number;
}

/** The forecast on screen, when it may stand as the series' last point. */
export interface NowSource {
  played: number;
  latest: string | null;
  teams: TeamRow[];
}

/** The settings the cached default forecast answers for. */
export interface Defaults {
  nSims: number;
  seed: number;
}

/** Whether `sim` is the default forecast: nothing assumed, no rating
 *  changed, no scenario, and the default simulations and seed. */
export function isDefaultForecast(sim: SimResponse, defaults: Defaults): boolean {
  return (
    sim.n_sims === defaults.nSims &&
    sim.seed === defaults.seed &&
    sim.what_if.length === 0 &&
    Object.keys(sim.elo_overrides ?? {}).length === 0 &&
    sim.scenario_applied == null
  );
}

/** The on-screen forecast as a final "now" point, or null. Only the default
 *  forecast qualifies, and only when the calendar holds more results than
 *  the last snapshot stood on: a what-if or a small run differs from the log
 *  for reasons that are not time, and would read as movement. */
export function nowSource(
  trend: TrendResponse | null,
  sim: SimResponse | null,
  asOf: AsOf | null,
  defaults: Defaults,
): NowSource | null {
  const last = trend?.snapshots[trend.snapshots.length - 1];
  if (!last || !sim || !asOf) return null;
  if (!isDefaultForecast(sim, defaults) || asOf.played <= last.played) return null;
  return { played: asOf.played, latest: asOf.latest_result, teams: sim.teams };
}

/** One club's points, oldest first: every snapshot that lists it, then the
 *  "now" point when there is one. */
export function clubSeries(
  snapshots: TrendSnapshot[],
  team: string,
  now: NowSource | null,
): TrendPoint[] {
  const out: TrendPoint[] = [];
  for (const s of snapshots) {
    const c = s.clubs.find((x) => x.team === team);
    if (!c) continue;
    out.push({
      played: s.played,
      latest: s.latest_result,
      now: false,
      reconstructed: s.reconstructed === true,
      title_pct: c.title_pct,
      top_four_pct: c.top_four_pct,
      relegation_pct: c.relegation_pct,
      exp_points: c.exp_points,
    });
  }
  const row = now?.teams.find((x) => x.team === team);
  if (now && row && out.length > 0) {
    out.push({
      played: now.played,
      latest: now.latest,
      now: true,
      reconstructed: false,
      title_pct: row.title_pct,
      top_four_pct: row.top_four_pct,
      relegation_pct: row.relegation_pct,
      exp_points: row.exp_points,
    });
  }
  return out;
}

/** Whether a series has anything to draw: two points, and some value above
 *  `floor`. The club dialog passes 0.05, so a chance that never printed as
 *  0.1% or more (a trace, or zero throughout) draws nothing. */
export function drawable(series: TrendPoint[], m: Measure, floor = 0): boolean {
  return series.length >= 2 && series.some((p) => p[m] > floor);
}

/** One scale for every row of a list, so rows compare: 0 to the highest
 *  value any of them reached over the season. */
export function sharedScale(series: TrendPoint[][], m: Measure): [number, number] {
  let hi = 0;
  for (const s of series) for (const p of s) hi = Math.max(hi, p[m]);
  return [0, Math.max(hi, 1)];
}

/** The fewest units a club-dialog sparkline spans, so noise stays flat. */
export const MIN_SPAN: Record<Measure, number> = {
  title_pct: 10,
  top_four_pct: 10,
  relegation_pct: 10,
  exp_points: 5,
};

/** Where a measure can go: chances 0-100, points from 0 up. */
const BOUNDS: Record<Measure, [number, number]> = {
  title_pct: [0, 100],
  top_four_pct: [0, 100],
  relegation_pct: [0, 100],
  exp_points: [0, Infinity],
};

/** A series' own scale: its range, widened about its middle to at least
 *  `MIN_SPAN` and kept inside the measure's bounds. */
export function ownScale(series: TrendPoint[], m: Measure): [number, number] {
  const values = series.map((p) => p[m]);
  let lo = Math.min(...values);
  let hi = Math.max(...values);
  const span = MIN_SPAN[m];
  if (hi - lo < span) {
    const mid = (lo + hi) / 2;
    lo = mid - span / 2;
    hi = mid + span / 2;
  }
  const [min, max] = BOUNDS[m];
  if (lo < min) {
    hi += min - lo;
    lo = min;
  }
  if (hi > max) {
    lo = Math.max(min, lo - (hi - max));
    hi = max;
  }
  return [lo, hi];
}

/** Pixel positions of a series in a `width` × `height` box, `pad` kept clear
 *  on every side for the end dot. x is matches played, so a "now" point a
 *  few results on sits a little past the last snapshot, not a full step. */
export function sparkPoints(
  series: TrendPoint[],
  m: Measure,
  scale: [number, number],
  width: number,
  height: number,
  pad: number,
): { x: number; y: number }[] {
  if (series.length === 0) return [];
  const first = series[0].played;
  const last = series[series.length - 1].played;
  const [lo, hi] = scale;
  const w = width - 2 * pad;
  const h = height - 2 * pad;
  const round = (v: number) => Math.round(v * 100) / 100;
  return series.map((p, i) => {
    const t = last > first ? (p.played - first) / (last - first) : series.length > 1 ? i / (series.length - 1) : 0;
    const v = hi > lo ? (Math.min(hi, Math.max(lo, p[m])) - lo) / (hi - lo) : 0;
    return { x: round(pad + t * w), y: round(pad + (1 - v) * h) };
  });
}

/** An SVG path through the points. */
export function sparkPath(points: { x: number; y: number }[]): string {
  return points.map((p, i) => `${i === 0 ? "M" : "L"}${p.x} ${p.y}`).join(" ");
}

/** A measure's value as the site prints it, zero included. */
export function formatValue(v: number, m: Measure, lang: Lang): string {
  return m === "exp_points" ? num(v, lang, 1) : pct(v, lang, 1, false);
}

/** Translated words for a series' text. */
export interface TrendWords {
  /** "{v} before the season". */
  before: string;
  /** "{v} after {date}" (the date of the latest result). */
  after: string;
  /** "{v} now". */
  now: string;
  /** "between {lo} and {hi}". */
  between: string;
}

/** A point's value with when it stood: "58.3% before the season". `current`
 *  is how many results the calendar holds; a point standing on all of them
 *  is "now". */
function pointText(p: TrendPoint, m: Measure, lang: Lang, words: TrendWords, current: number | null): string {
  const v = formatValue(p[m], m, lang);
  if (p.now || (current !== null && p.played === current && p.played > 0)) return fill(words.now, { v });
  if (p.played === 0 || p.latest === null) return fill(words.before, { v });
  return fill(words.after, { v, date: shortDate(p.latest, lang) });
}

/** Where the series started, for the caption under a club-dialog figure. */
export function startCaption(series: TrendPoint[], m: Measure, lang: Lang, words: TrendWords): string {
  return series.length ? pointText(series[0], m, lang, words, null) : "";
}

/** The sparkline's text alternative: "58.3% before the season, 84.4% now,
 *  between 58.3% and 84.4%". The range is left out when it prints as one
 *  value. */
export function sparkLabel(
  series: TrendPoint[],
  m: Measure,
  lang: Lang,
  words: TrendWords,
  current: number | null,
): string {
  if (series.length === 0) return "";
  const parts = [pointText(series[0], m, lang, words, current)];
  if (series.length > 1) parts.push(pointText(series[series.length - 1], m, lang, words, current));
  const values = series.map((p) => p[m]);
  const lo = formatValue(Math.min(...values), m, lang);
  const hi = formatValue(Math.max(...values), m, lang);
  if (lo !== hi) parts.push(fill(words.between, { lo, hi }));
  return parts.join(", ");
}

/** How much of the series was computed after the fact. Reconstructed
 *  snapshots lead the log; `until` names the latest result the last of them
 *  stood on (null for the preseason alone). With a "now" point after an
 *  all-reconstructed log, the log is "until" its end, since the now point
 *  was not computed afterwards. */
export type Reconstruction =
  | { kind: "none" }
  | { kind: "all" }
  | { kind: "until"; date: string | null };

export function reconstruction(snapshots: TrendSnapshot[], hasNow: boolean): Reconstruction {
  let last = -1;
  snapshots.forEach((s, i) => {
    if (s.reconstructed === true) last = i;
  });
  if (last < 0) return { kind: "none" };
  if (last === snapshots.length - 1 && !hasNow) return { kind: "all" };
  return { kind: "until", date: snapshots[last].latest_result };
}

/** Translated words for the reconstruction note. */
export interface ReconstructionWords {
  /** "Points up to {date} were computed afterwards; later ones are the
   *  forecast as published." */
  until: string;
  /** The same when only the preseason point was. */
  untilPreseason: string;
  /** Every point was. */
  all: string;
}

/** The sentence that discloses reconstruction, or "" when there is none. */
export function reconstructionNote(r: Reconstruction, lang: Lang, words: ReconstructionWords): string {
  if (r.kind === "none") return "";
  if (r.kind === "all") return words.all;
  return r.date === null ? words.untilPreseason : fill(words.until, { date: shortDate(r.date, lang) });
}

/** Everything the pages need to draw the trend. */
export interface TrendView {
  snapshots: TrendSnapshot[];
  /** The on-screen forecast as a last point, when it qualifies. */
  now: NowSource | null;
  /** Results the calendar holds while the default forecast is on screen,
   *  so a point standing on all of them reads "now"; null otherwise. */
  current: number | null;
}

/** The trend as the pages draw it; null when there is none to draw. */
export function trendView(
  trend: TrendResponse | null,
  sim: SimResponse | null,
  asOf: AsOf | null,
  defaults: Defaults,
): TrendView | null {
  if (!trend || trend.snapshots.length === 0) return null;
  const current = sim && asOf && isDefaultForecast(sim, defaults) ? asOf.played : null;
  return { snapshots: trend.snapshots, now: nowSource(trend, sim, asOf, defaults), current };
}

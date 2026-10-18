/* Pure helpers behind the forecast history in AccuracyPanel, kept apart from
   React so history.test.mjs tests them directly. */

import type { ScoredMatch } from "./api";
// With the extension: node's test runner loads this file as it is.
import { kickoffOrder } from "./clubs.ts";

/** One matchday of scored forecasts. */
export interface MatchdayHistory {
  round: number;
  /** By date, then home club. */
  matches: ScoredMatch[];
  n: number;
  /** Matches where the likeliest outcome happened. */
  hits: number;
  /** Mean log-loss of the model's forecasts on this matchday. */
  logLoss: number;
  /** The same for the base-rate forecast. */
  baselineLogLoss: number;
}

/** Where the season-to-date score stood after one matchday. */
export interface RunningPoint {
  round: number;
  /** Matches scored up to and including this matchday. */
  n: number;
  /** Mean log-loss over every match scored so far. */
  logLoss: number;
  baselineLogLoss: number;
}

const sum = (xs: number[]) => xs.reduce((a, b) => a + b, 0);

/** Within a matchday: the calendar's own order (`kickoffOrder`; a scored
 *  match carries its date but no kick-off time), then home club in Turkish
 *  alphabetical order (Ç after C, Ş after S). */
export function matchOrder(a: ScoredMatch, b: ScoredMatch): number {
  return kickoffOrder(a, b) || a.home.localeCompare(b.home, "tr");
}

/** Scored matches grouped by matchday, the latest matchday first. */
export function byMatchday(matches: ScoredMatch[]): MatchdayHistory[] {
  const rounds = new Map<number, ScoredMatch[]>();
  for (const m of matches) {
    const list = rounds.get(m.round);
    if (list) list.push(m);
    else rounds.set(m.round, [m]);
  }
  return [...rounds]
    .sort((a, b) => b[0] - a[0])
    .map(([round, list]) => {
      const sorted = [...list].sort(matchOrder);
      return {
        round,
        matches: sorted,
        n: sorted.length,
        hits: sorted.filter((m) => m.hit).length,
        logLoss: sum(sorted.map((m) => m.log_loss)) / sorted.length,
        baselineLogLoss: sum(sorted.map((m) => m.baseline_log_loss)) / sorted.length,
      };
    });
}

/** The season-to-date mean log-loss of the model and of the base rates after
 *  each matchday, earliest first. The last point is the report's headline. */
export function runningSeries(matches: ScoredMatch[]): RunningPoint[] {
  let n = 0;
  let loss = 0;
  let baseline = 0;
  return byMatchday(matches)
    .reverse()
    .map((day) => {
      n += day.n;
      loss += sum(day.matches.map((m) => m.log_loss));
      baseline += sum(day.matches.map((m) => m.baseline_log_loss));
      return { round: day.round, n, logLoss: loss / n, baselineLogLoss: baseline / n };
    });
}

/** Fills `{name}` placeholders in a translated string; unknown names stay. */
export function fill(template: string, values: Record<string, string>): string {
  return template.replace(/\{(\w+)\}/g, (whole, key: string) => values[key] ?? whole);
}

/** Round value-axis ticks covering every value, about `target` of them, on
 *  steps of 1, 2 or 5 times a power of ten. */
export function valueTicks(values: number[], target = 4): number[] {
  const finite = values.filter(Number.isFinite);
  if (finite.length === 0) return [];
  let lo = Math.min(...finite);
  let hi = Math.max(...finite);
  if (hi - lo < 1e-9) {
    lo -= 0.05;
    hi += 0.05;
  }
  const raw = (hi - lo) / Math.max(1, target - 1);
  const power = 10 ** Math.floor(Math.log10(raw));
  // The round step closest to the raw one, by ratio.
  const step = [1, 2, 5, 10]
    .map((f) => f * power)
    .reduce((best, s) => (Math.abs(Math.log(s / raw)) < Math.abs(Math.log(best / raw)) ? s : best));
  const first = Math.floor(lo / step + 1e-9);
  const last = Math.ceil(hi / step - 1e-9);
  const ticks: number[] = [];
  for (let i = first; i <= last; i++) ticks.push(Number((i * step).toFixed(10)));
  return ticks;
}

/** Decimals that print every tick of `valueTicks` exactly: 0.05 steps need
 *  two, 0.5 steps one. */
export function tickDigits(ticks: number[]): number {
  if (ticks.length < 2) return 2;
  const step = ticks[1] - ticks[0];
  return Math.max(0, Math.ceil(-Math.log10(step) - 1e-9));
}

/** Where a readout `tipWidth` wide starts so it sits `gap` to the right of
 *  a crosshair at `at`, or to its left when the right has no room, and never
 *  past either edge of a plot `width` wide. */
export function readoutLeft(at: number, tipWidth: number, width: number, gap = 12): number {
  const beside = at + gap + tipWidth <= width ? at + gap : at - gap - tipWidth;
  return Math.max(0, Math.min(beside, width - tipWidth));
}

/** Matchday-axis ticks from `first` to `last` on a 1, 2, 5 or 10 step, so
 *  labels at least `minGap` pixels apart fit across `width`. */
export function roundTicks(first: number, last: number, width: number, minGap = 28): number[] {
  if (last <= first) return [first];
  const perRound = width / (last - first);
  const step = [1, 2, 5, 10].find((s) => s * perRound >= minGap) ?? 10;
  const ticks: number[] = [];
  for (let r = Math.ceil(first / step) * step; r <= last; r += step) ticks.push(r);
  return ticks.length > 0 ? ticks : [first, last];
}

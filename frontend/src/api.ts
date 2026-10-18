import { sendWithRetry } from "./retry";

export interface TeamRow {
  team: string;
  title_pct: number;
  title_odds: number | null;
  top_two_pct: number;
  third_pct: number;
  fourth_pct: number;
  top_four_pct: number;
  relegation_pct: number;
  relegation_odds: number | null;
  exp_points: number;
  exp_gd: number;
  mean_position: number;
}

export interface PositionRow {
  team: string;
  /** position_pct[i] = P(finishing in position i + 1); sums to 100. */
  position_pct: number[];
}

export interface TableRow {
  position: number;
  team: string;
  played: number;
  won: number;
  drawn: number;
  lost: number;
  gf: number;
  ga: number;
  gd: number;
  points: number;
}

export interface RivalryPair {
  a: string;
  b: string;
  a_above_pct: number;
  count: number;
}

export interface RaceThreshold {
  position: number;
  label: string;
  /** Points that took the place in 50 / 75 / 90% of simulated seasons. */
  p50: number;
  p75: number;
  p90: number;
}

export interface SimResponse {
  n_sims: number;
  seed: number;
  teams: TeamRow[];
  positions: PositionRow[];
  table: TableRow[];
  rivalries: RivalryPair[];
  consensus_champion: string;
  thresholds: RaceThreshold[];
  what_if: WhatIf[];
  elo_overrides: Record<string, number>;
  scenario_applied: string | null;
}

export interface WhatIf {
  home: string;
  away: string;
  /** "home" | "draw" | "away" */
  outcome: string;
}

export interface SimRequest {
  n_sims?: number;
  seed?: number;
  elo_overrides?: Record<string, number>;
  what_if?: WhatIf[];
}

export interface ScenarioRequest {
  prompt: string;
  n_sims?: number;
  seed?: number;
}

const API_BASE = "";

export { ApiError } from "./retry";

/** Every call goes through here: a busy server (503, 429) is asked again
 *  after the wait retry.ts derives from its Retry-After, and any other
 *  failure throws ApiError with the backend's message. */
async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const resp = await sendWithRetry(() => fetch(`${API_BASE}${path}`, init), {
    signal: init.signal ?? undefined,
  });
  return resp.json();
}

function postJson(body: unknown, signal?: AbortSignal): RequestInit {
  return {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
    signal,
  };
}

/** The default forecast (50,000 seasons, seed 12345, nothing assumed). The
 *  server computes it once per state of the league, so a first visit never
 *  waits on a simulation of its own. Same JSON as runSimulation({}). */
export async function getForecast(signal?: AbortSignal): Promise<SimResponse> {
  return request("/api/forecast", { signal });
}

export async function runSimulation(req: SimRequest, signal?: AbortSignal): Promise<SimResponse> {
  return request("/api/simulate", postJson(req, signal));
}

export async function runScenario(req: ScenarioRequest): Promise<SimResponse> {
  return request("/api/scenario", postJson(req));
}

export interface PlayedMatch {
  round: number;
  home: string;
  home_score: number;
  away: string;
  away_score: number;
}

export interface LiveData {
  played_matches: PlayedMatch[];
  fetched_at: string;
}

export async function refreshLiveData(): Promise<LiveData> {
  return request("/api/refresh", { method: "POST" });
}

/** Cached live data (kept fresh by the backend's background refresh). */
export async function getLiveData(): Promise<LiveData | null> {
  return request("/api/live");
}

export interface UpcomingMatch {
  round: number;
  home: string;
  away: string;
  home_win_pct: number;
  draw_pct: number;
  away_win_pct: number;
  home_odds: number | null;
  draw_odds: number | null;
  away_odds: number | null;
}

export interface UpcomingResponse {
  matches: UpcomingMatch[];
}

export interface MatchForecast {
  home_win_pct: number;
  draw_pct: number;
  away_win_pct: number;
  home_odds: number | null;
  draw_odds: number | null;
  away_odds: number | null;
  over25_pct: number;
  over25_odds: number | null;
  under25_odds: number | null;
  btts_pct: number;
  btts_odds: number | null;
  btts_no_odds: number | null;
  /** The model's expected goals (λ) for each side. */
  home_xg: number;
  away_xg: number;
  /** The three most probable exact scorelines, most likely first. */
  likely_scores: LikelyScore[];
}

export interface LikelyScore {
  home: number;
  away: number;
  pct: number;
}

export interface MatchCard {
  home: string;
  away: string;
  /** Kick-off date, ISO YYYY-MM-DD. */
  date: string;
  /** Kick-off time HH:MM, once TFF publishes it. */
  kickoff: string | null;
  played: boolean;
  home_score: number | null;
  away_score: number | null;
  forecast: MatchForecast | null;
}

export interface RoundMatches {
  round: number;
  matches: MatchCard[];
}

export interface MatchesResponse {
  rounds: RoundMatches[];
  /** The next matchday, the UI's landing round, and the round
   *  /api/upcoming lists (World::next_round_on): the round of the earliest
   *  unplayed fixture dated today or later (Istanbul time), so one
   *  postponed match does not hold it back. If two rounds have unplayed
   *  fixtures on that date, the one with more fixtures that day wins.
   *  With only overdue fixtures left, it is the earliest round still
   *  holding one; once every fixture is played, the last round. */
  current_round: number;
}

/** The full 306-fixture calendar with model prices for every unplayed game. */
export async function getMatches(): Promise<MatchesResponse> {
  return request("/api/matches");
}

/** Win/draw/loss forecasts for the next matchday's unplayed fixtures. */
export async function getUpcoming(): Promise<UpcomingResponse> {
  return request("/api/upcoming");
}

export interface ScoredMatch {
  round: number;
  date: string;
  home: string;
  away: string;
  home_score: number;
  away_score: number;
  /** Probability the model gave the outcome that actually happened. */
  outcome_pct: number;
  /** "1" home win, "X" draw, "2" away win. */
  outcome: "1" | "X" | "2";
  /** Whether the model's likeliest outcome was the one that happened. */
  hit: boolean;
  /** The frozen forecast, exactly as logged. */
  home_pct: number;
  draw_pct: number;
  away_pct: number;
  /** The day the forecast was written down. */
  logged_at: string;
  /** This match's -ln p of the true outcome; the report's log_loss is
   *  the mean of these. */
  log_loss: number;
  /** The base-rate forecast's -ln p for the same outcome. */
  baseline_log_loss: number;
}

export interface CalibrationBucket {
  band_from_pct: number;
  band_to_pct: number;
  predictions: number;
  mean_predicted_pct: number;
  actual_pct: number;
}

export interface AccuracyReport {
  scored: number;
  hit_rate_pct: number;
  log_loss: number;
  baseline_log_loss: number;
  calibration: CalibrationBucket[];
  matches: ScoredMatch[];
}

/** How the model has done against predictions frozen before kick-off. */
export async function getAccuracy(): Promise<AccuracyReport> {
  return request("/api/accuracy");
}

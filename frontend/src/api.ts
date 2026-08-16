export interface TeamRow {
  team: string;
  title_pct: number;
  title_odds: number | null;
  ucl_pct: number;
  uel_pct: number;
  uecl_pct: number;
  europe_pct: number;
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

export interface SimResponse {
  n_sims: number;
  seed: number;
  teams: TeamRow[];
  positions: PositionRow[];
  table: TableRow[];
  rivalries: RivalryPair[];
  consensus_champion: string;
  elo_overrides: Record<string, number>;
  scenario_applied: string | null;
}

export interface SimRequest {
  n_sims?: number;
  seed?: number;
  elo_overrides?: Record<string, number>;
}

export interface ScenarioRequest {
  prompt: string;
  n_sims?: number;
  seed?: number;
}

const API_BASE = "";

export async function runSimulation(req: SimRequest): Promise<SimResponse> {
  const resp = await fetch(`${API_BASE}/api/simulate`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
  if (!resp.ok) throw new Error(await resp.text());
  return resp.json();
}

export async function runScenario(req: ScenarioRequest): Promise<SimResponse> {
  const resp = await fetch(`${API_BASE}/api/scenario`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
  if (!resp.ok) throw new Error(await resp.text());
  return resp.json();
}

export interface LiveData {
  played_matches: {
    round: number;
    home: string;
    home_score: number;
    away: string;
    away_score: number;
  }[];
  fetched_at: string;
}

export async function refreshLiveData(): Promise<LiveData> {
  const resp = await fetch(`${API_BASE}/api/refresh`, { method: "POST" });
  if (!resp.ok) throw new Error(await resp.text());
  return resp.json();
}

/** Cached live data (kept fresh by the backend's background refresh). */
export async function getLiveData(): Promise<LiveData | null> {
  const resp = await fetch(`${API_BASE}/api/live`);
  if (!resp.ok) throw new Error(await resp.text());
  return resp.json();
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

/** Win/draw/loss forecasts for the next matchday's unplayed fixtures. */
export async function getUpcoming(): Promise<UpcomingResponse> {
  const resp = await fetch(`${API_BASE}/api/upcoming`);
  if (!resp.ok) throw new Error(await resp.text());
  return resp.json();
}

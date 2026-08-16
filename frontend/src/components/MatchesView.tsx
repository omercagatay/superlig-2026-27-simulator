import { useState } from "react";
import type { MatchesResponse, MatchCard } from "../api";

const fmtOdds = (o: number | null) => (o != null ? o.toFixed(2) : "–");

function ForecastRow({ m }: { m: MatchCard }) {
  const f = m.forecast;
  if (!f) return null;
  const homeFavored = f.home_win_pct >= f.away_win_pct;
  return (
    <div className="match-card">
      <div className="fixture-teams">
        <span className={`fixture-team${homeFavored ? " favored" : ""}`}>{m.home}</span>
        <span className="fixture-vs">v</span>
        <span className={`fixture-team${homeFavored ? "" : " favored"}`}>{m.away}</span>
      </div>
      <div className="split-bar">
        <div className="split-a" style={{ width: `${f.home_win_pct}%` }} />
        <div className="split-d" style={{ width: `${f.draw_pct}%` }} />
        <div className="split-b" style={{ width: `${f.away_win_pct}%` }} />
      </div>
      <div className="odds-grid">
        <span className="odds-cell">
          <span className="odds-label">1</span>
          {f.home_win_pct.toFixed(1)}% <em>{fmtOdds(f.home_odds)}</em>
        </span>
        <span className="odds-cell">
          <span className="odds-label">X</span>
          {f.draw_pct.toFixed(1)}% <em>{fmtOdds(f.draw_odds)}</em>
        </span>
        <span className="odds-cell">
          <span className="odds-label">2</span>
          {f.away_win_pct.toFixed(1)}% <em>{fmtOdds(f.away_odds)}</em>
        </span>
        <span className="odds-cell">
          <span className="odds-label">O2.5</span>
          {f.over25_pct.toFixed(1)}% <em>{fmtOdds(f.over25_odds)}</em>
        </span>
        <span className="odds-cell">
          <span className="odds-label">U2.5</span>
          {(100 - f.over25_pct).toFixed(1)}% <em>{fmtOdds(f.under25_odds)}</em>
        </span>
        <span className="odds-cell">
          <span className="odds-label">BTTS</span>
          {f.btts_pct.toFixed(1)}% <em>{fmtOdds(f.btts_odds)}</em>
        </span>
      </div>
    </div>
  );
}

function PlayedRow({ m }: { m: MatchCard }) {
  return (
    <div className="match-card match-played">
      <div className="fixture-teams">
        <span className="fixture-team">{m.home}</span>
        <span className="result-score">
          {m.home_score}–{m.away_score}
        </span>
        <span className="fixture-team">{m.away}</span>
      </div>
      <span className="match-ft">FT</span>
    </div>
  );
}

export function MatchesView({ data }: { data: MatchesResponse }) {
  const [round, setRound] = useState(data.current_round);
  const total = data.rounds.length;
  const current = data.rounds.find((r) => r.round === round) ?? data.rounds[0];

  return (
    <section className="panel" aria-label="Match predictions">
      <header className="panel-head">
        <h2>Match predictions</h2>
        <div className="round-nav" role="group" aria-label="Matchday">
          <button
            type="button"
            className="btn"
            onClick={() => setRound(Math.max(1, round - 1))}
            disabled={round <= 1}
            aria-label="Previous matchday"
          >
            ‹
          </button>
          <label className="round-pick">
            <select value={round} onChange={(e) => setRound(Number(e.target.value))}>
              {data.rounds.map((r) => (
                <option key={r.round} value={r.round}>
                  Matchday {r.round}
                </option>
              ))}
            </select>
          </label>
          <button
            type="button"
            className="btn"
            onClick={() => setRound(Math.min(total, round + 1))}
            disabled={round >= total}
            aria-label="Next matchday"
          >
            ›
          </button>
        </div>
      </header>
      <p className="panel-note">
        Probabilities are exact under the model's scoreline distribution; odds
        are the fair price 100/p with no bookmaker margin. Model estimates, not
        betting advice.
      </p>
      <div className="match-list">
        {current.matches.map((m) =>
          m.played ? (
            <PlayedRow key={`${m.home}-${m.away}`} m={m} />
          ) : (
            <ForecastRow key={`${m.home}-${m.away}`} m={m} />
          )
        )}
      </div>
    </section>
  );
}

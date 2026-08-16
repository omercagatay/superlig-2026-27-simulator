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
      <div className="xg-line">
        xG {f.home_xg.toFixed(2)} – {f.away_xg.toFixed(2)}
      </div>
      <div className="split-bar">
        <div className="split-a" style={{ width: `${f.home_win_pct}%` }} />
        <div className="split-d" style={{ width: `${f.draw_pct}%` }} />
        <div className="split-b" style={{ width: `${f.away_win_pct}%` }} />
      </div>
      <table className="market-table">
        <thead>
          <tr>
            <th scope="col" className="mt-label">
              Market
            </th>
            <th scope="col">Probability</th>
            <th scope="col">Fair odds</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td className="mt-label">
              <i className="split-key a" aria-hidden="true" />
              {m.home} win <span className="mt-code">1</span>
            </td>
            <td>{f.home_win_pct.toFixed(1)}%</td>
            <td>{fmtOdds(f.home_odds)}</td>
          </tr>
          <tr>
            <td className="mt-label">
              <i className="split-key d" aria-hidden="true" />
              Draw <span className="mt-code">X</span>
            </td>
            <td>{f.draw_pct.toFixed(1)}%</td>
            <td>{fmtOdds(f.draw_odds)}</td>
          </tr>
          <tr className="mt-group-end">
            <td className="mt-label">
              <i className="split-key b" aria-hidden="true" />
              {m.away} win <span className="mt-code">2</span>
            </td>
            <td>{f.away_win_pct.toFixed(1)}%</td>
            <td>{fmtOdds(f.away_odds)}</td>
          </tr>
          <tr>
            <td className="mt-label">Over 2.5 goals</td>
            <td>{f.over25_pct.toFixed(1)}%</td>
            <td>{fmtOdds(f.over25_odds)}</td>
          </tr>
          <tr>
            <td className="mt-label">Under 2.5 goals</td>
            <td>{(100 - f.over25_pct).toFixed(1)}%</td>
            <td>{fmtOdds(f.under25_odds)}</td>
          </tr>
          <tr>
            <td className="mt-label">Both teams score</td>
            <td>{f.btts_pct.toFixed(1)}%</td>
            <td>{fmtOdds(f.btts_odds)}</td>
          </tr>
        </tbody>
      </table>
      <div className="score-chips">
        <span className="score-chips-label">Most likely scores</span>
        {f.likely_scores.map((sc) => (
          <span key={`${sc.home}-${sc.away}`} className="score-chip">
            {sc.home}–{sc.away} <em>{sc.pct.toFixed(1)}%</em>
          </span>
        ))}
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
        Every unplayed fixture is priced on match result (1X2), total goals and
        both-teams-to-score: the probability of each outcome and its fair
        decimal odds (100/probability, no bookmaker margin). Matches already
        played show the final score instead of a forecast. Model estimates, not
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

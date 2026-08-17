import { useState } from "react";
import type { MatchesResponse, MatchCard } from "../api";

const fmtOdds = (o: number | null) => (o != null ? o.toFixed(2) : "–");

/** "Sat 22 Aug" in the reader's locale; the ISO date is date-only, so parse
 *  it as local midnight rather than letting UTC shift it a day. */
function fmtDate(iso: string): string {
  const [y, m, d] = iso.split("-").map(Number);
  if (!y || !m || !d) return iso;
  return new Date(y, m - 1, d).toLocaleDateString(undefined, {
    weekday: "short",
    day: "numeric",
    month: "short",
  });
}

/** The date span a matchday covers — Süper Lig rounds run Fri to Mon. */
function roundSpan(matches: { date: string }[]): string {
  const dates = [...new Set(matches.map((m) => m.date))].sort();
  if (dates.length === 0) return "";
  const first = fmtDate(dates[0]);
  const last = fmtDate(dates[dates.length - 1]);
  return first === last ? first : `${first} – ${last}`;
}

function ForecastRow({ m }: { m: MatchCard }) {
  const f = m.forecast;
  if (!f) return null;
  const homeFavored = f.home_win_pct >= f.away_win_pct;
  const mk = m.market;
  return (
    <div className="match-card">
      <div className="fixture-teams">
        <span className={`fixture-team${homeFavored ? " favored" : ""}`}>{m.home}</span>
        <span className="fixture-vs">v</span>
        <span className={`fixture-team${homeFavored ? "" : " favored"}`}>{m.away}</span>
      </div>
      <div className="xg-line">
        <span className="match-when">
          {fmtDate(m.date)}
          {m.kickoff ? ` · ${m.kickoff}` : ""}
        </span>
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
            {mk && <th scope="col">Bookmaker</th>}
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
            {mk && <td className="mt-book">{mk.home_odds.toFixed(2)}</td>}
          </tr>
          <tr>
            <td className="mt-label">
              <i className="split-key d" aria-hidden="true" />
              Draw <span className="mt-code">X</span>
            </td>
            <td>{f.draw_pct.toFixed(1)}%</td>
            <td>{fmtOdds(f.draw_odds)}</td>
            {mk && <td className="mt-book">{mk.draw_odds.toFixed(2)}</td>}
          </tr>
          <tr className="mt-group-end">
            <td className="mt-label">
              <i className="split-key b" aria-hidden="true" />
              {m.away} win <span className="mt-code">2</span>
            </td>
            <td>{f.away_win_pct.toFixed(1)}%</td>
            <td>{fmtOdds(f.away_odds)}</td>
            {mk && <td className="mt-book">{mk.away_odds.toFixed(2)}</td>}
          </tr>
          <tr>
            <td className="mt-label">Over 2.5 goals</td>
            <td>{f.over25_pct.toFixed(1)}%</td>
            <td>{fmtOdds(f.over25_odds)}</td>
            {mk && <td />}
          </tr>
          <tr>
            <td className="mt-label">Under 2.5 goals</td>
            <td>{(100 - f.over25_pct).toFixed(1)}%</td>
            <td>{fmtOdds(f.under25_odds)}</td>
            {mk && <td />}
          </tr>
          <tr>
            <td className="mt-label">Both teams score</td>
            <td>{f.btts_pct.toFixed(1)}%</td>
            <td>{fmtOdds(f.btts_odds)}</td>
            {mk && <td />}
          </tr>
        </tbody>
      </table>
      {mk && (
        <div className="edge-line">
          <span className="edge-label">vs bookmaker</span>
          <span className={`edge-val${mk.edge_pct >= 0 ? " edge-up" : " edge-down"}`}>
            {mk.edge_pct >= 0 ? "+" : ""}
            {mk.edge_pct.toFixed(1)} pts on {mk.edge_outcome}
          </span>
          <span className="edge-margin">book margin {((mk.overround - 1) * 100).toFixed(1)}%</span>
        </div>
      )}
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

/** Played fixtures collapse into one compact block — a result is a fact,
 *  not a forecast, and should not compete with the priced cards. */
function PlayedBlock({ matches }: { matches: MatchCard[] }) {
  if (matches.length === 0) return null;
  return (
    <div className="ft-block">
      <span className="ft-block-label">Final scores</span>
      <div className="ft-rows">
        {matches.map((m) => (
          <span key={`${m.home}-${m.away}`} className="ft-row">
            <span className="ft-date">{fmtDate(m.date)}</span> {m.home}{" "}
            <span className="result-score">
              {m.home_score}–{m.away_score}
            </span>{" "}
            {m.away}
          </span>
        ))}
      </div>
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
        <span className="eyebrow">{roundSpan(current.matches)}</span>
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
      <div className="panel-body">
        <p className="panel-note">
          Fair odds are 100 / probability, with no bookmaker margin. Model
          estimates, not betting advice.
        </p>
        <PlayedBlock matches={current.matches.filter((m) => m.played)} />
        <div className="match-list">
          {current.matches
            .filter((m) => !m.played)
            .map((m) => (
              <ForecastRow key={`${m.home}-${m.away}`} m={m} />
            ))}
        </div>
      </div>
    </section>
  );
}

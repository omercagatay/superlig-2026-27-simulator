import { useState } from "react";
import type { SimResponse, LiveData, MatchesResponse, AccuracyReport } from "../api";
import { ResultsTable } from "./ResultsTable";
import { MatchesView } from "./MatchesView";
import { AccuracyPanel } from "./AccuracyPanel";
import { ClubDetail } from "./ClubDetail";

const TOTAL_FIXTURES = 306;

export function ForecastView({
  data,
  liveData,
  matchesData,
  accuracy,
  liveMatchCount,
  onShowLive,
}: {
  data: SimResponse;
  liveData: LiveData | null;
  matchesData: MatchesResponse | null;
  accuracy: AccuracyReport | null;
  liveMatchCount: number;
  onShowLive: () => void;
}) {
  const [selected, setSelected] = useState<string | null>(null);
  const selectedTeam = data.teams.find((t) => t.team === selected);
  const contenders = data.teams.filter((t) => t.title_pct > 0).slice(0, 6);
  const maxTitle = contenders[0]?.title_pct ?? 1;
  const favorite = data.teams[0];
  const riskiest = [...data.teams].sort((a, b) => b.relegation_pct - a.relegation_pct)[0];
  const nextRound = matchesData?.current_round;
  const nextUnplayed = matchesData?.rounds
    .find((r) => r.round === nextRound)
    ?.matches.filter((m) => !m.played).length;

  return (
    <div className="forecast">
      {/* One summary strip; every number below it earns its own panel. */}
      <div className="tiles">
        <div className="tile">
          <span className="tile-label">Title favorite</span>
          <span className="tile-value">{favorite.team}</span>
          <span className="tile-sub">
            {favorite.title_pct.toFixed(1)}% of seasons
            {favorite.title_odds != null && ` · fair odds ${favorite.title_odds.toFixed(2)}`}
          </span>
        </div>
        <div className="tile">
          <span className="tile-label">Relegation risk</span>
          <span className="tile-value">{riskiest.team}</span>
          <span className="tile-sub">{riskiest.relegation_pct.toFixed(1)}% of seasons</span>
        </div>
        <div className="tile">
          <span className="tile-label">Season progress</span>
          <span className="tile-value">
            {liveMatchCount}
            <span className="tile-dim"> / {TOTAL_FIXTURES}</span>
          </span>
          <span className="tile-bar" aria-hidden="true">
            <span
              className="tile-bar-fill"
              style={{ width: `${(liveMatchCount / TOTAL_FIXTURES) * 100}%` }}
            />
          </span>
        </div>
        <div className="tile">
          <span className="tile-label">Next matchday</span>
          <span className="tile-value">{nextRound != null ? `MD ${nextRound}` : "—"}</span>
          <span className="tile-sub">
            {nextUnplayed != null
              ? `${nextUnplayed} fixture${nextUnplayed === 1 ? "" : "s"} to play`
              : "calendar loading"}
          </span>
        </div>
      </div>

      <div className="forecast-grid">
        <div className="forecast-main">
          <ResultsTable
            teams={data.teams}
            nSims={data.n_sims}
            seed={data.seed}
            onSelect={setSelected}
          />
        </div>

        <aside className="rail">
          <section className="panel" aria-label="Title race">
            <header className="panel-head">
              <h2>Title race</h2>
            </header>
            {contenders.map((t, i) => (
              <div key={t.team} className={`race-row${i === 0 ? " race-leader" : ""}`}>
                <span className="race-rank">{i + 1}</span>
                <div>
                  <div className="race-top">
                    <span className="race-team">{t.team}</span>
                  </div>
                  <div className="race-meter">
                    <div
                      className="race-fill"
                      style={{ width: `${(t.title_pct / maxTitle) * 100}%` }}
                    />
                  </div>
                </div>
                <span className="race-pct">
                  {t.title_pct.toFixed(1)}
                  <span className="pct-sign">%</span>
                </span>
              </div>
            ))}
          </section>

          {accuracy && <AccuracyPanel report={accuracy} />}

          {data.rivalries.length > 0 && (
            <section className="panel" aria-label="Head-to-head finishing order">
              <header className="panel-head">
                <h3>Who finishes above whom</h3>
              </header>
              {data.rivalries.slice(0, 5).map((r, i) => (
                <div key={i} className="finals-row">
                  <span className="finals-pair">
                    <strong>{r.a}</strong> above <strong>{r.b}</strong>
                  </span>
                  <span className="finals-pct">{r.a_above_pct.toFixed(1)}%</span>
                </div>
              ))}
              {liveData && (
                <button type="button" className="panel-foot-link" onClick={onShowLive}>
                  All live results →
                </button>
              )}
            </section>
          )}
        </aside>
      </div>

      {matchesData && <MatchesView data={matchesData} />}

      {selectedTeam && (
        <ClubDetail
          team={selectedTeam}
          position={data.positions.find((p) => p.team === selectedTeam.team)}
          matches={matchesData}
          onClose={() => setSelected(null)}
        />
      )}
    </div>
  );
}

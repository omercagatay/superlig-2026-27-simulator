import type { SimResponse, LiveData, UpcomingMatch } from "../api";
import { ScenarioPrompt } from "./ScenarioPrompt";
import { ResultsTable } from "./ResultsTable";

export function ForecastView({
  data,
  liveData,
  upcoming,
  loading,
  onScenario,
  liveMatchCount,
  onShowLive,
}: {
  data: SimResponse;
  liveData: LiveData | null;
  upcoming: UpcomingMatch[];
  loading: boolean;
  onScenario: (prompt: string) => void;
  liveMatchCount: number;
  onShowLive: () => void;
}) {
  const contenders = data.teams.filter((t) => t.title_pct > 0).slice(0, 6);
  const maxTitle = contenders[0]?.title_pct ?? 1;
  const overrides = Object.entries(data.elo_overrides);
  const nextRound = upcoming[0]?.round;

  return (
    <div className="forecast">
      {data.scenario_applied && (
        <div className="scenario-note">
          <strong>Scenario:</strong> {data.scenario_applied}
          {overrides.length > 0 && (
            <div className="elo-chips">
              {overrides.map(([team, elo]) => (
                <span key={team} className="elo-chip">
                  {team} → {elo.toFixed(0)}
                </span>
              ))}
            </div>
          )}
        </div>
      )}

      <div className="forecast-grid">
        <div className="forecast-main">
          <section className="panel" aria-label="Title race">
            <header className="panel-head">
              <h2>Title race</h2>
              <span className="eyebrow">
                {data.n_sims.toLocaleString()} seasons · seed {data.seed}
              </span>
            </header>
            {contenders.map((t, i) => (
              <div key={t.team} className={`race-row${i === 0 ? " race-leader" : ""}`}>
                <span className="race-rank">{i + 1}</span>
                <div>
                  <div className="race-top">
                    <span className="race-team">{t.team}</span>
                    {i === 0 && <span className="tag-champ">Most likely champion</span>}
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

          <ResultsTable teams={data.teams} />
        </div>

        <aside className="rail">
          <ScenarioPrompt onSubmit={onScenario} disabled={loading} />

          {upcoming.length > 0 && (
            <section className="panel" aria-label="Next matchday">
              <header className="panel-head">
                <h3>Matchday {nextRound}</h3>
              </header>
              {upcoming.map((m) => (
                <div key={`${m.home}-${m.away}`} className="fixture">
                  <div className="fixture-teams">
                    <span
                      className={`fixture-team${
                        m.home_win_pct >= m.away_win_pct ? " favored" : ""
                      }`}
                    >
                      {m.home}
                    </span>
                    <span className="fixture-vs">v</span>
                    <span
                      className={`fixture-team${
                        m.away_win_pct > m.home_win_pct ? " favored" : ""
                      }`}
                    >
                      {m.away}
                    </span>
                  </div>
                  {/* Three-way split: home / draw / away. A league match can
                      end level, so the draw gets its own segment. */}
                  <div className="split-bar">
                    <div className="split-a" style={{ width: `${m.home_win_pct}%` }} />
                    <div className="split-d" style={{ width: `${m.draw_pct}%` }} />
                    <div className="split-b" style={{ width: `${m.away_win_pct}%` }} />
                  </div>
                  <div className="split-labels">
                    <span>
                      <i className="split-key a" aria-hidden="true" />
                      {m.home_win_pct.toFixed(1)}%
                    </span>
                    <span>
                      <i className="split-key d" aria-hidden="true" />
                      {m.draw_pct.toFixed(1)}%
                    </span>
                    <span>
                      <i className="split-key b" aria-hidden="true" />
                      {m.away_win_pct.toFixed(1)}%
                    </span>
                  </div>
                </div>
              ))}
            </section>
          )}

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
            </section>
          )}

          {liveData && (
            <section className="panel" aria-label="Season so far">
              <header className="panel-head">
                <h3>Season so far</h3>
              </header>
              <div className="snapshot-rows">
                <div className="snapshot-row">
                  <span>Matches played</span>
                  <strong>{liveMatchCount}</strong>
                </div>
                <div className="snapshot-row">
                  <span>Matches remaining</span>
                  <strong>{306 - liveMatchCount}</strong>
                </div>
                <div className="snapshot-row">
                  <span>Projected champion</span>
                  <strong>{data.consensus_champion}</strong>
                </div>
              </div>
              <button type="button" className="panel-foot-link" onClick={onShowLive}>
                All live results →
              </button>
            </section>
          )}
        </aside>
      </div>
    </div>
  );
}

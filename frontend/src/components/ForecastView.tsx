import type { SimResponse, LiveData, MatchesResponse } from "../api";
import { ResultsTable } from "./ResultsTable";
import { MatchesView } from "./MatchesView";

export function ForecastView({
  data,
  liveData,
  matchesData,
  liveMatchCount,
  onShowLive,
}: {
  data: SimResponse;
  liveData: LiveData | null;
  matchesData: MatchesResponse | null;
  liveMatchCount: number;
  onShowLive: () => void;
}) {
  const contenders = data.teams.filter((t) => t.title_pct > 0).slice(0, 6);
  const maxTitle = contenders[0]?.title_pct ?? 1;

  return (
    <div className="forecast">
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

      {matchesData && <MatchesView data={matchesData} />}
    </div>
  );
}

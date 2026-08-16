import type { TeamRow } from "../api";

/* Heat tint for probability cells: one sequential hue (turf), opacity scaled
   by the value so magnitude reads at a glance. Mixed from the token so it
   tracks the active theme. */
const heat = (pct: number) =>
  pct > 0
    ? {
        background: `color-mix(in srgb, var(--turf) ${(5 + (pct / 100) * 22).toFixed(1)}%, transparent)`,
      }
    : undefined;

/* Relegation risk reads in the danger hue, not the turf hue — it is a
   different kind of outcome and should not look like a good one. */
const risk = (pct: number) =>
  pct > 0
    ? {
        background: `color-mix(in srgb, var(--danger) ${(5 + (pct / 100) * 22).toFixed(1)}%, transparent)`,
      }
    : undefined;

const Pct = ({ v }: { v: number }) =>
  v > 0 ? <>{v.toFixed(1)}</> : <span className="cell-zero">–</span>;

export function ResultsTable({
  teams,
  nSims,
  seed,
}: {
  teams: TeamRow[];
  nSims: number;
  seed: number;
}) {
  const maxTitle = Math.max(...teams.map((t) => t.title_pct), 0.001);

  return (
    <section className="panel table-panel" aria-label="Season outcome probabilities">
      <header className="panel-head">
        <h2>Season outcomes</h2>
        <span className="eyebrow">
          {nSims.toLocaleString()} seasons · seed {seed}
        </span>
      </header>
      <div className="table-scroll">
        <table className="data-table">
          <thead>
            <tr>
              <th aria-label="Rank">#</th>
              <th className="col-team">Club</th>
              <th className="cell-title">Title</th>
              <th title="Decimal odds">Odds</th>
              <th title="Champions League (1st-2nd)">UCL</th>
              <th title="Europa League (3rd)">UEL</th>
              <th title="Conference League (4th)">UECL</th>
              <th title="Any European place (1st-4th)">Europe</th>
              <th title="Finishes in the bottom three">Relegation</th>
              <th title="Expected points over 34 matches">xPts</th>
              <th title="Expected goal difference">xGD</th>
            </tr>
          </thead>
          <tbody>
            {teams.map((t, i) => (
              <tr key={t.team}>
                <td className="cell-rank">{i + 1}</td>
                <td className="cell-team">{t.team}</td>
                <td className="cell-title">
                  {t.title_pct > 0 ? (
                    <div className="title-meter">
                      <div className="title-track">
                        <div
                          className="title-fill"
                          style={{ width: `${(t.title_pct / maxTitle) * 100}%` }}
                        />
                      </div>
                      <span className="title-val">{t.title_pct.toFixed(1)}%</span>
                    </div>
                  ) : (
                    <span className="cell-zero">–</span>
                  )}
                </td>
                <td>
                  {t.title_odds != null ? (
                    t.title_odds.toFixed(2)
                  ) : (
                    <span className="cell-zero">–</span>
                  )}
                </td>
                <td style={heat(t.ucl_pct)}>
                  <Pct v={t.ucl_pct} />
                </td>
                <td style={heat(t.uel_pct)}>
                  <Pct v={t.uel_pct} />
                </td>
                <td style={heat(t.uecl_pct)}>
                  <Pct v={t.uecl_pct} />
                </td>
                <td style={heat(t.europe_pct)}>
                  <Pct v={t.europe_pct} />
                </td>
                <td style={risk(t.relegation_pct)}>
                  <Pct v={t.relegation_pct} />
                </td>
                <td>{t.exp_points.toFixed(1)}</td>
                <td>
                  {t.exp_gd > 0 ? "+" : ""}
                  {t.exp_gd.toFixed(1)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

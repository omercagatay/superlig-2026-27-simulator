import type { TeamRow } from "../api";
import { useT } from "../i18n";

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
  onSelect,
}: {
  teams: TeamRow[];
  nSims: number;
  seed: number;
  onSelect: (team: string) => void;
}) {
  const tr = useT();
  const maxTitle = Math.max(...teams.map((t) => t.title_pct), 0.001);

  return (
    <section className="panel table-panel" aria-label="Season outcome probabilities">
      <header className="panel-head">
        <h2>{tr("seasonOutcomes")}</h2>
        <span className="eyebrow">{tr("clickAClub")}</span>
        <span className="eyebrow">
          {nSims.toLocaleString()} {tr("seasonsSeed")} {seed}
        </span>
      </header>
      <div className="table-scroll">
        <table className="data-table">
          <thead>
            <tr>
              <th aria-label="Rank">#</th>
              <th className="col-team">{tr("club")}</th>
              <th className="cell-title">{tr("title")}</th>
              <th title={tr("tipDecimalOdds")}>{tr("odds")}</th>
              <th title={tr("tipUcl")}>UCL</th>
              <th title={tr("tipUel")}>UEL</th>
              <th title={tr("tipUecl")}>UECL</th>
              <th title={tr("tipEurope")}>{tr("europe")}</th>
              <th title={tr("tipRelegation")}>{tr("relegation")}</th>
              <th title={tr("tipXpts")}>{tr("xPts")}</th>
              <th title={tr("tipXgd")}>{tr("xGD")}</th>
            </tr>
          </thead>
          <tbody>
            {teams.map((t, i) => (
              <tr
                key={t.team}
                className="row-clickable"
                tabIndex={0}
                role="button"
                aria-label={`${t.team} detail`}
                onClick={() => onSelect(t.team)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    onSelect(t.team);
                  }
                }}
              >
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

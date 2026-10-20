import type { ReactNode } from "react";
import type { TableRow } from "../api";
import { useLang, useT } from "../i18n";
import { signed } from "../clubs";
import { ClubName } from "../components/Common";

export type TableView = "current" | "projected";

/** One league table, with zone marks for the top two, 3rd–4th and the
 *  bottom three. `playedOnNarrow` keeps the games-played column on a phone,
 *  where the current table needs it: postponed matches leave clubs on
 *  different counts. */
function LeagueTable({
  rows,
  playedOnNarrow,
  onSelectClub,
}: {
  rows: TableRow[];
  playedOnNarrow: boolean;
  onSelectClub: (team: string) => void;
}) {
  const t = useT();
  const { lang } = useLang();
  const n = rows.length;
  const zone = (pos: number) =>
    pos <= 2 ? "zone-top2" : pos <= 4 ? "zone-top4" : pos > n - 3 ? "zone-rel" : undefined;
  const playedClass = playedOnNarrow ? undefined : "wide-only";

  return (
    <div className="table-wrap">
      <table className="data league">
        <thead>
          <tr>
            <th scope="col">{t("colPos")}</th>
            <th scope="col" className="l">
              {t("club")}
            </th>
            <th scope="col" className={playedClass}>
              {t("colP")}
            </th>
            <th scope="col">{t("colW")}</th>
            <th scope="col">{t("colD")}</th>
            <th scope="col">{t("colL")}</th>
            <th scope="col" className="wide-only">{t("colF")}</th>
            <th scope="col" className="wide-only">{t("colA")}</th>
            <th scope="col">{t("colGD")}</th>
            <th scope="col">{t("colPts")}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.team} className={zone(r.position)}>
              <td className="rank">{r.position}</td>
              <td className="l">
                <button type="button" className="row-link" onClick={() => onSelectClub(r.team)}>
                  <ClubName team={r.team} />
                </button>
              </td>
              <td className={playedClass}>{r.played}</td>
              <td>{r.won}</td>
              <td>{r.drawn}</td>
              <td>{r.lost}</td>
              <td className="wide-only">{r.gf}</td>
              <td className="wide-only">{r.ga}</td>
              <td>{signed(r.gd, lang, 0)}</td>
              <td className="pts">{r.points}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** The table as it stands and the projected final table, one at a time
 *  behind a switch. Each comes from its own request (the calendar, the
 *  simulation), so one can show while the other is missing. */
export function TablePage({
  current,
  projected,
  view,
  onView,
  currentMissing,
  projectedMissing,
  onSelectClub,
}: {
  current: TableRow[] | null;
  projected: TableRow[] | null;
  view: TableView;
  onView: (view: TableView) => void;
  /** Shown in place of a table that is not there yet, or failed. */
  currentMissing: ReactNode;
  projectedMissing: ReactNode;
  onSelectClub: (team: string) => void;
}) {
  const t = useT();
  const rows = view === "current" ? current : projected;

  return (
    <>
      <h1 className="page-title">{t(view === "current" ? "currentTable" : "projectedTable")}</h1>
      <div className="segmented table-switch" role="group" aria-label={t("pageTable")}>
        <button type="button" aria-pressed={view === "current"} onClick={() => onView("current")}>
          {t("tableCurrent")}
        </button>
        <button type="button" aria-pressed={view === "projected"} onClick={() => onView("projected")}>
          {t("tableProjected")}
        </button>
      </div>
      {rows ? (
        <>
          <LeagueTable rows={rows} playedOnNarrow={view === "current"} onSelectClub={onSelectClub} />
          <ul className="legend">
            <li>
              <span className="key" /> {t("topTwo")}
            </li>
            <li>
              <span className="key key-soft" /> {t("thirdFourth")}
            </li>
            <li>
              <span className="key key-red" /> {t("relegation")}
            </li>
          </ul>
        </>
      ) : view === "current" ? (
        currentMissing
      ) : (
        projectedMissing
      )}
    </>
  );
}

import type { TableRow } from "../api";
import { useLang, useT } from "../i18n";
import { signed } from "../clubs";
import { ClubName, PageHead } from "../components/Print";

/** Page 3: the projected final table, set like a printed league table —
 *  thin rules under 2nd and 4th, a red rule above the bottom three. */
export function TablePage({
  table,
  onSelectClub,
}: {
  table: TableRow[];
  onSelectClub: (team: string) => void;
}) {
  const t = useT();
  const { lang } = useLang();
  const n = table.length;
  const rowClass = (pos: number) =>
    [pos === 2 || pos === 4 ? "rule-after" : "", pos === n - 2 ? "rule-red" : ""]
      .filter(Boolean)
      .join(" ");

  return (
    <article className="page">
      <PageHead title={t("pageTable")} page={3} />
      <p className="note">
        {t("projectedNote")} {t("tapForClub")}
      </p>
      <div className="table-frame">
        <table className="print league">
          <caption className="sr-only">{t("projectedTable")}</caption>
          <thead>
            <tr>
              <th scope="col">{t("colPos")}</th>
              <th scope="col" className="l">
                {t("club")}
              </th>
              <th scope="col">{t("colP")}</th>
              <th scope="col">{t("colW")}</th>
              <th scope="col">{t("colD")}</th>
              <th scope="col">{t("colL")}</th>
              <th scope="col">{t("colF")}</th>
              <th scope="col">{t("colA")}</th>
              <th scope="col">{t("colGD")}</th>
              <th scope="col">{t("colPts")}</th>
            </tr>
          </thead>
          <tbody>
            {table.map((r) => (
              <tr key={r.team} className={rowClass(r.position)}>
                <td className="rank">{r.position}</td>
                <td className="l">
                  <button type="button" className="row-link" onClick={() => onSelectClub(r.team)}>
                    <ClubName team={r.team} />
                  </button>
                </td>
                <td>{r.played}</td>
                <td>{r.won}</td>
                <td>{r.drawn}</td>
                <td>{r.lost}</td>
                <td>{r.gf}</td>
                <td>{r.ga}</td>
                <td>{signed(r.gd, lang, 0)}</td>
                <td className="pts">{r.points}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <ul className="legend">
        <li>
          <span className="key" /> {t("topTwo")} · {t("topFour")}
        </li>
        <li>
          <span className="key key-red" /> {t("relegation")}
        </li>
      </ul>
    </article>
  );
}

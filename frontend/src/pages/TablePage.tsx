import type { TableRow } from "../api";
import { useLang, useT } from "../i18n";
import { signed } from "../clubs";
import { ClubName } from "../components/Common";

/** The projected final table, with zone marks for the top two, 3rd–4th
 *  and the bottom three. */
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
  const zone = (pos: number) =>
    pos <= 2 ? "zone-top2" : pos <= 4 ? "zone-top4" : pos > n - 3 ? "zone-rel" : undefined;

  return (
    <>
      <h1 className="page-title">{t("projectedTable")}</h1>
      <div className="table-wrap">
        <table className="data league">
          <thead>
            <tr>
              <th scope="col">{t("colPos")}</th>
              <th scope="col" className="l">
                {t("club")}
              </th>
              <th scope="col" className="wide-only">{t("colP")}</th>
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
            {table.map((r) => (
              <tr key={r.team} className={zone(r.position)}>
                <td className="rank">{r.position}</td>
                <td className="l">
                  <button type="button" className="row-link" onClick={() => onSelectClub(r.team)}>
                    <ClubName team={r.team} />
                  </button>
                </td>
                <td className="wide-only">{r.played}</td>
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
  );
}

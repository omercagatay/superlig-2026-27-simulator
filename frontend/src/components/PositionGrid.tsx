import type { CSSProperties } from "react";
import type { PositionRow } from "../api";
import { useLang, useT } from "../i18n";
import { pct } from "../clubs";
import { ClubName } from "./Common";

/** Square-root ramp so small but real chances stay visible. */
const intensity = (p: number) => Math.min(1, Math.sqrt(p / 100));

/** The 18×18 finishing-position grid as a heat map. The tint tops out at
 *  --heat-max, which keeps every figure at 4.5:1 contrast in both themes. */
export function PositionGrid({ positions }: { positions: PositionRow[] }) {
  const t = useT();
  const { lang } = useLang();
  const total = positions.length;
  const places = Array.from({ length: total }, (_, i) => i + 1);

  return (
    <section>
      <h2 className="section-title">{t("finishingPositions")}</h2>
      <div className="table-wrap">
        <table className="posgrid">
          <thead>
            <tr>
              <th scope="col">
                <span className="sr-only">{t("club")}</span>
              </th>
              {places.map((p) => (
                <th
                  scope="col"
                  key={p}
                  className={p <= 4 ? "top4" : p > total - 3 ? "rel" : undefined}
                >
                  {p}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {positions.map((row) => (
              <tr key={row.team}>
                <th scope="row">
                  <ClubName team={row.team} />
                </th>
                {row.position_pct.map((p, i) => (
                  <td
                    key={i}
                    style={{ "--i": intensity(p) } as CSSProperties}
                    title={`${row.team} · ${i + 1}. · ${pct(p, lang)}`}
                  >
                    {p >= 1 ? Math.round(p) : ""}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <ul className="legend">
        <li>
          <span className="key" /> {t("topFourPlaces")}
        </li>
        <li>
          <span className="key key-red" /> {t("relegationZone")}
        </li>
      </ul>
    </section>
  );
}

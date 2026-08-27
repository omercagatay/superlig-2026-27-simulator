import type { CSSProperties } from "react";
import type { PositionRow } from "../api";
import { useT } from "../i18n";

function intensity(pct: number): number {
  // Square-root ramp so small but real probabilities stay visible.
  return Math.min(1, Math.sqrt(pct / 100));
}

function cellTitle(team: string, position: number, pct: number): string {
  return `${team} finishes ${position}${ordinal(position)}: ${pct.toFixed(1)}%`;
}

function ordinal(n: number): string {
  if (n % 10 === 1 && n % 100 !== 11) return "st";
  if (n % 10 === 2 && n % 100 !== 12) return "nd";
  if (n % 10 === 3 && n % 100 !== 13) return "rd";
  return "th";
}

export function PositionGrid({ positions }: { positions: PositionRow[] }) {
  const tr = useT();
  const total = positions.length;
  const columns = Array.from({ length: total }, (_, i) => i + 1);

  return (
    <section className="panel" aria-label="Finishing position probabilities">
      <header className="panel-head">
        <h2>{tr("finishingPositions")}</h2>
      </header>
      <p className="panel-note">
        {tr("finishingNote")}
      </p>
      <div className="grid-scroll">
        <table className="position-grid">
          <thead>
            <tr>
              <th scope="col">{tr("club")}</th>
              {columns.map((c) => (
                <th
                  scope="col"
                  key={c}
                  className={
                    c <= 4 ? "col-europe" : c > total - 3 ? "col-rel" : undefined
                  }
                >
                  {c}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {positions.map((row) => (
              <tr key={row.team}>
                <th scope="row">{row.team}</th>
                {row.position_pct.map((pct, i) => (
                  <td
                    key={i}
                    className="grid-cell"
                    style={{ "--i": intensity(pct) } as CSSProperties}
                    title={cellTitle(row.team, i + 1, pct)}
                  >
                    {pct >= 1 ? Math.round(pct) : ""}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <ul className="legend">
        <li>
          <span className="swatch col-europe" /> {tr("topFourPlaces")}
        </li>
        <li>
          <span className="swatch col-rel" /> {tr("relegationZone")}
        </li>
      </ul>
    </section>
  );
}

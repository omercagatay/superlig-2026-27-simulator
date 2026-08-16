import type { TableRow } from "../api";

/** Marks the European and relegation cut lines in the projected table. */
function zoneClass(position: number, total: number): string {
  if (position <= 2) return "zone-ucl";
  if (position === 3) return "zone-uel";
  if (position === 4) return "zone-uecl";
  if (position > total - 3) return "zone-rel";
  return "";
}

export function LeagueTable({ table }: { table: TableRow[] }) {
  return (
    <section className="panel" aria-label="Projected final table">
      <header className="panel-head">
        <h2>Projected final table</h2>
      </header>
      <p className="panel-note">
        Each club's expected record, averaged over every simulated season and
        ranked by expected points — the table the model considers most likely,
        not one sampled run.
      </p>
      <div className="table-scroll">
        <table className="data-table">
          <thead>
            <tr>
              <th scope="col">#</th>
              <th scope="col" className="col-team">
                Club
              </th>
              <th scope="col">P</th>
              <th scope="col">W</th>
              <th scope="col">D</th>
              <th scope="col">L</th>
              <th scope="col">GF</th>
              <th scope="col">GA</th>
              <th scope="col">GD</th>
              <th scope="col">Pts</th>
            </tr>
          </thead>
          <tbody>
            {table.map((r) => (
              <tr key={r.team} className={zoneClass(r.position, table.length)}>
                <td className="cell-rank">{r.position}</td>
                <td className="cell-team">{r.team}</td>
                <td>{r.played}</td>
                <td>{r.won}</td>
                <td>{r.drawn}</td>
                <td>{r.lost}</td>
                <td>{r.gf}</td>
                <td>{r.ga}</td>
                <td>{r.gd > 0 ? `+${r.gd}` : r.gd}</td>
                <td>
                  <strong>{r.points}</strong>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <ul className="legend">
        <li>
          <span className="swatch zone-ucl" /> Champions League
        </li>
        <li>
          <span className="swatch zone-uel" /> Europa League
        </li>
        <li>
          <span className="swatch zone-uecl" /> Conference League
        </li>
        <li>
          <span className="swatch zone-rel" /> Relegation
        </li>
      </ul>
    </section>
  );
}

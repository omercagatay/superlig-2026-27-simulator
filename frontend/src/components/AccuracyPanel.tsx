import type { AccuracyReport } from "../api";

/** How the model has actually performed against predictions it committed to
 *  before kick-off. An empty log is the honest state at the start of a
 *  season, not an error — say so rather than showing zeros. */
export function AccuracyPanel({ report }: { report: AccuracyReport }) {
  if (report.scored === 0) {
    return (
      <section className="panel" aria-label="Forecast accuracy">
        <header className="panel-head">
          <h3>Forecast accuracy</h3>
        </header>
        <p className="panel-note">
          Predictions are frozen before kick-off and scored once the matches
          are played. Nothing to score yet — the tracker fills in from the next
          completed matchday.
        </p>
      </section>
    );
  }

  const edge = report.baseline_log_loss - report.log_loss;
  return (
    <section className="panel" aria-label="Forecast accuracy">
      <header className="panel-head">
        <h3>Forecast accuracy</h3>
        <span className="eyebrow">{report.scored} scored</span>
      </header>
      <div className="snapshot-rows">
        <div className="snapshot-row">
          <span>Called correctly</span>
          <strong>{report.hit_rate_pct.toFixed(0)}%</strong>
        </div>
        <div className="snapshot-row">
          <span>Log-loss</span>
          <strong>{report.log_loss.toFixed(3)}</strong>
        </div>
        <div className="snapshot-row">
          <span>vs base rates</span>
          <strong className={edge >= 0 ? "edge-up" : "edge-down"}>
            {edge >= 0 ? "−" : "+"}
            {Math.abs(edge).toFixed(3)}
          </strong>
        </div>
      </div>
      {report.calibration.length > 0 && (
        <div className="calib">
          <span className="score-chips-label">Calibration</span>
          {report.calibration.map((b) => (
            <div key={b.band_from_pct} className="calib-row">
              <span className="calib-band">
                {b.band_from_pct}–{b.band_to_pct}%
              </span>
              <span className="calib-bar" aria-hidden="true">
                <span className="calib-pred" style={{ width: `${b.mean_predicted_pct}%` }} />
                <span className="calib-act" style={{ width: `${b.actual_pct}%` }} />
              </span>
              <span className="calib-val">
                said {b.mean_predicted_pct.toFixed(0)}% · happened {b.actual_pct.toFixed(0)}%
              </span>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

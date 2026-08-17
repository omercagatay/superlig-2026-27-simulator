import type { AccuracyReport } from "../api";
import { useT } from "../i18n";

/** How the model has actually performed against predictions it committed to
 *  before kick-off. An empty log is the honest state at the start of a
 *  season, not an error — say so rather than showing zeros. */
export function AccuracyPanel({ report }: { report: AccuracyReport }) {
  const tr = useT();
  if (report.scored === 0) {
    return (
      <section className="panel" aria-label="Forecast accuracy">
        <header className="panel-head">
          <h3>{tr("forecastAccuracy")}</h3>
        </header>
        <p className="panel-note">{tr("accuracyEmpty")}</p>
      </section>
    );
  }

  const edge = report.baseline_log_loss - report.log_loss;
  return (
    <section className="panel" aria-label="Forecast accuracy">
      <header className="panel-head">
        <h3>{tr("forecastAccuracy")}</h3>
        <span className="eyebrow">
          {report.scored} {tr("scored")}
        </span>
      </header>
      <div className="snapshot-rows">
        <div className="snapshot-row">
          <span>{tr("calledCorrectly")}</span>
          <strong>{report.hit_rate_pct.toFixed(0)}%</strong>
        </div>
        <div className="snapshot-row">
          <span>{tr("logLoss")}</span>
          <strong>{report.log_loss.toFixed(3)}</strong>
        </div>
        <div className="snapshot-row">
          <span>{tr("vsBaseRates")}</span>
          <strong className={edge >= 0 ? "edge-up" : "edge-down"}>
            {edge >= 0 ? "−" : "+"}
            {Math.abs(edge).toFixed(3)}
          </strong>
        </div>
      </div>
      {report.calibration.length > 0 && (
        <div className="calib">
          <span className="score-chips-label">{tr("calibration")}</span>
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
                {tr("said")} {b.mean_predicted_pct.toFixed(0)}% · {tr("happened")}{" "}
                {b.actual_pct.toFixed(0)}%
              </span>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

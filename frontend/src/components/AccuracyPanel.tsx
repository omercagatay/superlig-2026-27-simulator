import type { AccuracyReport } from "../api";
import { useLang, useT } from "../i18n";
import { num, pct, signed } from "../clubs";
import { Bar } from "./Common";

/** How the model did against predictions frozen before kick-off. An empty
 *  log is the honest state early in a season. */
export function AccuracyPanel({ report }: { report: AccuracyReport }) {
  const t = useT();
  const { lang } = useLang();
  const band = (from: number, to: number) =>
    lang === "tr" ? `%${from}–${to}` : `${from}–${to}%`;

  return (
    <section>
      <h2 className="section-title">{t("accuracyTitle")}</h2>
      {report.scored === 0 ? (
        <p className="muted">{t("accuracyEmpty")}</p>
      ) : (
        <>
          <dl className="facts">
            <div>
              <dt>{t("scoredMatches")}</dt>
              <dd>{num(report.scored, lang)}</dd>
            </div>
            <div>
              <dt>{t("calledCorrectly")}</dt>
              <dd>{pct(report.hit_rate_pct, lang, 0, false)}</dd>
            </div>
            <div>
              <dt>{t("logLoss")}</dt>
              <dd>{num(report.log_loss, lang, 3)}</dd>
            </div>
            <div>
              <dt>{t("vsBaseRates")}</dt>
              <dd>{signed(report.log_loss - report.baseline_log_loss, lang, 3)}</dd>
            </div>
          </dl>
          {report.calibration.length > 0 && (
            <ul className="calib" aria-label={t("calibration")}>
              {report.calibration.map((b) => (
                <li key={b.band_from_pct}>
                  <span>{band(b.band_from_pct, b.band_to_pct)}</span>
                  <span className="calib-bars">
                    <Bar value={b.mean_predicted_pct} />
                    <Bar value={b.actual_pct} tone="muted" />
                  </span>
                  <span className="calib-val">
                    {t("predicted")} {pct(b.mean_predicted_pct, lang, 0, false)}, {t("happened")}{" "}
                    {pct(b.actual_pct, lang, 0, false)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    </section>
  );
}

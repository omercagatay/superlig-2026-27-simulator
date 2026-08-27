import type { CouponSelection, DailyCouponResponse } from "../api";
import { useLocale, useT } from "../i18n";

function formatDate(iso: string, locale?: string): string {
  const [year, month, day] = iso.split("-").map(Number);
  if (!year || !month || !day) return iso;
  return new Date(year, month - 1, day).toLocaleDateString(locale, {
    weekday: "short",
    day: "numeric",
    month: "short",
  });
}

function formatTimestamp(value: string | null, locale?: string): string {
  const timestamp = Date.parse(value ?? "");
  if (!Number.isFinite(timestamp)) return "—";
  return new Date(timestamp).toLocaleString(locale, {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function pickName(selection: CouponSelection, draw: string): string {
  if (selection.outcome === "1") return `${selection.home} ${selection.outcome}`;
  if (selection.outcome === "2") return `${selection.away} ${selection.outcome}`;
  return `${draw} X`;
}

function CouponCard({ selection, index }: { selection: CouponSelection; index: number }) {
  const t = useT();
  const locale = useLocale();
  return (
    <article className="coupon-card">
      <div className="coupon-card-top">
        <span className="coupon-leg">{index + 1}</span>
        <span className="coupon-when">
          {formatDate(selection.date, locale)}
          {selection.kickoff ? ` · ${selection.kickoff}` : ""}
        </span>
        <span className="coupon-round">
          {t("mdShort")} {selection.round}
        </span>
      </div>
      <div className="coupon-fixture">
        <span>{selection.home}</span>
        <span className="fixture-vs">v</span>
        <span>{selection.away}</span>
      </div>
      <div className="coupon-pick-row">
        <div>
          <span className="coupon-stat-label">{t("couponPick")}</span>
          <strong className="coupon-pick">{pickName(selection, t("draw"))}</strong>
        </div>
        <div className="coupon-price">
          <span className="coupon-stat-label">{t("bookmaker")}</span>
          <strong>{selection.market_odds.toFixed(2)}</strong>
        </div>
      </div>
      <dl className="coupon-stats">
        <div>
          <dt>{t("couponModelChance")}</dt>
          <dd>{selection.model_pct.toFixed(1)}%</dd>
        </div>
        <div>
          <dt>{t("couponMarketChance")}</dt>
          <dd>{selection.market_pct.toFixed(1)}%</dd>
        </div>
        <div>
          <dt>{t("couponEdge")}</dt>
          <dd className="coupon-positive">+{selection.edge_pct.toFixed(1)} {t("couponPoints")}</dd>
        </div>
      </dl>
    </article>
  );
}

function StatusMessage({ status }: { status: DailyCouponResponse["status"] }) {
  const t = useT();
  const key =
    status === "no_value"
      ? "couponNoValue"
      : status === "market_stale"
        ? "couponStale"
        : "couponUnavailable";
  return (
    <div className="coupon-empty" role="status">
      <span className="coupon-empty-mark" aria-hidden="true">—</span>
      <h2>{t(key)}</h2>
      <p>{t(`${key}Body` as `${typeof key}Body`)}</p>
    </div>
  );
}

export function DailyCouponView({
  data,
  loading,
  error,
}: {
  data: DailyCouponResponse | null;
  loading: boolean;
  error: string | null;
}) {
  const t = useT();
  const locale = useLocale();

  if (loading && !data) {
    return (
      <section className="panel coupon-loading" aria-live="polite">
        <div className="boot-spinner" aria-hidden="true" />
        <p>{t("couponLoading")}</p>
      </section>
    );
  }

  if (!data) {
    return (
      <section className="panel coupon-loading" role="alert">
        <p>{error || t("couponUnavailableBody")}</p>
      </section>
    );
  }

  const ready = data.status === "ready" && data.selections.length > 0;
  const from = data.window_from ? formatDate(data.window_from, locale) : null;
  const to = data.window_to ? formatDate(data.window_to, locale) : null;
  const windowLabel = from && to ? (from === to ? from : `${from} – ${to}`) : "—";

  return (
    <div className="daily-coupon">
      <section className="panel coupon-hero">
        <div className="coupon-title-row">
          <div>
            <span className="eyebrow">{t("couponEyebrow")}</span>
            <h2>{t("dailyCoupon")}</h2>
          </div>
          <div className="coupon-badges" aria-label={t("couponGuardrails")}>
            <span className="coupon-badge coupon-badge-age">18+</span>
            <span className="coupon-badge">{t("couponLicensedOnly")}</span>
            <span className="coupon-badge">{t("couponNoAffiliate")}</span>
          </div>
        </div>

        {ready ? (
          <div className="coupon-summary">
            <div className="coupon-summary-main">
              <span>{t("couponCombinedOdds")}</span>
              <strong>{data.combined_odds?.toFixed(2)}</strong>
            </div>
            <div>
              <span>{t("couponLegs")}</span>
              <strong>{data.selections.length}</strong>
            </div>
            <div>
              <span>{t("couponModelJoint")}</span>
              <strong>{data.combined_model_pct?.toFixed(1)}%</strong>
            </div>
            <div>
              <span>{t("couponWindow")}</span>
              <strong>{windowLabel}</strong>
            </div>
          </div>
        ) : (
          <StatusMessage status={data.status} />
        )}

        <p className="coupon-warning">
          <strong>{t("couponWarningLead")}</strong> {t("couponWarning")}
        </p>
      </section>

      {ready && (
        <section aria-label={t("dailyCoupon")}>
          <div className="coupon-grid">
            {data.selections.map((selection, index) => (
              <CouponCard
                key={`${selection.home}-${selection.away}-${selection.outcome}`}
                selection={selection}
                index={index}
              />
            ))}
          </div>
          <p className="coupon-joint-note">{t("couponJointNote")}</p>
        </section>
      )}

      <div className="coupon-info-grid">
        <section className="panel">
          <header className="panel-head">
            <h3>{t("couponHowSelected")}</h3>
          </header>
          <div className="panel-body coupon-method">
            <p>{t("couponMethodBody")}</p>
            <ul>
              <li>{t("couponRuleProbability")}</li>
              <li>{t("couponRuleEdge")}</li>
              <li>{t("couponRuleValue")}</li>
              <li>{t("couponRulePrice")}</li>
            </ul>
            <p className="coupon-source-line">
              {t("couponOddsSource")}: {data.source.odds_provider} · {t("fetched")} {formatTimestamp(data.market_fetched_at, locale)}
            </p>
          </div>
        </section>

        <section className="panel">
          <header className="panel-head">
            <h3>{t("couponLicensedOperators")}</h3>
          </header>
          <div className="panel-body">
            <p className="panel-note coupon-operator-note">{t("couponOperatorNote")}</p>
            <div className="operator-links">
              {data.licensed_operators.map((operator) => (
                <a
                  key={operator.name}
                  href={operator.url}
                  target="_blank"
                  rel="noopener noreferrer nofollow"
                  className="operator-link"
                >
                  {operator.name}<span aria-hidden="true">↗</span>
                </a>
              ))}
            </div>
            <div className="coupon-authority-links">
              <a href={data.source.regulator_url} target="_blank" rel="noopener noreferrer">
                {data.source.regulator} ↗
              </a>
              <span>{t("couponVerified")} {data.source.operator_verified_at}</span>
            </div>
          </div>
        </section>
      </div>

      <aside className="responsible-strip">
        <strong>{t("responsiblePlay")}</strong>
        <span>{t("responsiblePlayBody")}</span>
        <a href="https://www.yedam.org.tr/bagimlilik-turleri/kumar-bagimliligi" target="_blank" rel="noopener noreferrer">
          YEDAM 115 ↗
        </a>
      </aside>
    </div>
  );
}

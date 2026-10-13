import { useLang, useT } from "../i18n";
import { stamp } from "../clubs";

export type Theme = "light" | "dark";

/** The printer's note: print run, reprint and refresh, the edition
 *  switches, and the not-advice line. */
export function Colophon({
  nSims,
  seed,
  onSims,
  onSeed,
  onReprint,
  onRefresh,
  loading,
  refreshing,
  fetchedAt,
  theme,
  onTheme,
}: {
  nSims: number;
  seed: number;
  onSims: (n: number) => void;
  onSeed: (n: number) => void;
  onReprint: () => void;
  onRefresh: () => void;
  loading: boolean;
  refreshing: boolean;
  fetchedAt: string | null;
  theme: Theme;
  onTheme: (t: Theme) => void;
}) {
  const t = useT();
  const { lang, setLang } = useLang();
  const updated = fetchedAt ? stamp(fetchedAt, lang) : null;

  return (
    <footer className="colophon">
      <form
        className="colophon-run"
        onSubmit={(e) => {
          e.preventDefault();
          onReprint();
        }}
      >
        <span className="colophon-label">{t("printRun")}:</span>
        <label>
          <input
            type="number"
            inputMode="numeric"
            min={100}
            max={200000}
            step={1000}
            value={nSims}
            aria-label={t("sims")}
            onChange={(e) => onSims(Number(e.target.value))}
          />{" "}
          {t("seasonsWord")}
        </label>
        <span aria-hidden="true">·</span>
        <label>
          {t("seed")}{" "}
          <input
            type="number"
            inputMode="numeric"
            value={seed}
            onChange={(e) => onSeed(Number(e.target.value))}
          />
        </label>
        {updated && (
          <span>
            · {t("lastUpdate")} {updated}
          </span>
        )}
        <span className="colophon-actions">
          <button type="submit" className="btn btn-red" disabled={loading}>
            {loading ? t("printing") : t("reprint")}
          </button>
          <button type="button" className="btn" onClick={onRefresh} disabled={refreshing}>
            {refreshing ? t("updating") : t("updateLive")}
          </button>
        </span>
      </form>
      <div className="colophon-toggles">
        <button
          type="button"
          className="btn"
          lang={lang === "tr" ? "en" : "tr"}
          onClick={() => setLang(lang === "tr" ? "en" : "tr")}
        >
          {lang === "tr" ? "English edition" : "Türkçe baskı"}
        </button>
        <button
          type="button"
          className="btn"
          aria-pressed={theme === "dark"}
          onClick={() => onTheme(theme === "dark" ? "light" : "dark")}
        >
          {t("nightEdition")}
        </button>
      </div>
      <p className="legal">
        {t("notAdvice")} {t("sourcesNote")}
      </p>
    </footer>
  );
}

import { useLang, useT } from "../i18n";
import { stamp } from "../clubs";

export type Theme = "light" | "dark";

/** Simulation settings, refresh, language and theme, and the disclaimer. */
export function Footer({
  nSims,
  seed,
  onSims,
  onSeed,
  onRun,
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
  onRun: () => void;
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
    <footer className="site-footer">
      <div className="site-footer-inner">
        <form
          className="settings"
          onSubmit={(e) => {
            e.preventDefault();
            onRun();
          }}
        >
          <label>
            {t("sims")}{" "}
            <input
              type="number"
              inputMode="numeric"
              min={100}
              max={200000}
              step={100}
              value={nSims}
              onChange={(e) => onSims(Number(e.target.value))}
            />
          </label>
          <label>
            {t("seed")}{" "}
            <input
              type="number"
              inputMode="numeric"
              value={seed}
              onChange={(e) => onSeed(Number(e.target.value))}
            />
          </label>
          <button type="submit" className="btn btn-primary" disabled={loading}>
            {loading ? t("running") : t("run")}
          </button>
          <button type="button" className="btn" onClick={onRefresh} disabled={refreshing}>
            {refreshing ? t("updating") : t("updateLive")}
          </button>
        </form>
        <div className="prefs">
          <button
            type="button"
            className="btn"
            lang={lang === "tr" ? "en" : "tr"}
            onClick={() => setLang(lang === "tr" ? "en" : "tr")}
          >
            {lang === "tr" ? "English" : "Türkçe"}
          </button>
          <button
            type="button"
            className="btn"
            aria-pressed={theme === "dark"}
            onClick={() => onTheme(theme === "dark" ? "light" : "dark")}
          >
            {t("darkMode")}
          </button>
        </div>
        <p className="fineprint">
          {updated && `${t("lastUpdate")}: ${updated}. `}
          {t("notAdvice")} {t("sourcesNote")}
        </p>
      </div>
    </footer>
  );
}

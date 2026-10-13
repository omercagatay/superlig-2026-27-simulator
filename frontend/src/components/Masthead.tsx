import { useLang, useT, type Key } from "../i18n";
import { dateSpan } from "../clubs";

export const PAGES = ["cover", "fixtures", "table", "races", "results"] as const;
export type Page = (typeof PAGES)[number];

const PAGE_KEY: Record<Page, Key> = {
  cover: "pageCover",
  fixtures: "pageFixtures",
  table: "pageTable",
  races: "pageRaces",
  results: "pageResults",
};

/** The page named in the URL hash, so the back button and shared links
 *  land on the right page. */
export function pageFromHash(): Page {
  const id = window.location.hash.slice(1);
  return (PAGES as readonly string[]).includes(id) ? (id as Page) : "cover";
}

/** The programme's masthead, and the contents strip that stays in reach
 *  while scrolling. */
export function Masthead({
  issue,
  dates,
  conditional,
  page,
  onGo,
}: {
  issue: number | null;
  dates: string[];
  conditional: boolean;
  page: Page;
  onGo: (p: Page) => void;
}) {
  const t = useT();
  const { lang } = useLang();
  return (
    <>
      <header className="masthead">
        <p className="mast-kicker" lang="tr">
          {t("mastKicker")}
        </p>
        <h1 className="mast-title">{t("mastTitle")}</h1>
        <p className="mast-issue">
          {issue != null && `${t("issue")} ${issue}`}
          {dates.length > 0 && ` · ${dateSpan(dates, lang)}`}
          {conditional && (
            <>
              {" · "}
              <span className="mast-flag">{t("conditionalEdition")}</span>
            </>
          )}
        </p>
      </header>
      <nav className="contents" aria-label={t("contents")}>
        <span className="contents-label">{t("contents")}</span>
        {PAGES.map((p, i) => (
          <button
            key={p}
            type="button"
            aria-current={page === p ? "page" : undefined}
            onClick={() => onGo(p)}
          >
            <span className="contents-no">{i + 1}</span>
            {t(PAGE_KEY[p])}
          </button>
        ))}
      </nav>
    </>
  );
}

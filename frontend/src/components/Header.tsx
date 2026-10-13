import type { MouseEvent } from "react";
import { useT, type Key } from "../i18n";

export const PAGES = ["overview", "fixtures", "table", "probabilities", "results"] as const;
export type Page = (typeof PAGES)[number];

const PAGE_KEY: Record<Page, Key> = {
  overview: "pageOverview",
  fixtures: "pageFixtures",
  table: "pageTable",
  probabilities: "pageProbabilities",
  results: "pageResults",
};

/** The page named in the URL hash, so the back button and shared links
 *  land on the right page. */
export function pageFromHash(): Page {
  const id = window.location.hash.slice(1);
  return (PAGES as readonly string[]).includes(id) ? (id as Page) : "overview";
}

/** Site name and page tabs, sticky so navigation stays in reach. */
export function Header({ page, onGo }: { page: Page; onGo: (p: Page) => void }) {
  const t = useT();
  const link = (p: Page) => ({
    href: `#${p}`,
    onClick: (e: MouseEvent) => {
      e.preventDefault();
      onGo(p);
    },
  });
  return (
    <header className="site-header">
      <div className="site-header-inner">
        <a className="site-name" {...link("overview")}>
          {t("siteName")}
        </a>
        <nav className="tabs" aria-label={t("pages")}>
          {PAGES.map((p) => (
            <a key={p} {...link(p)} aria-current={page === p ? "page" : undefined}>
              {t(PAGE_KEY[p])}
            </a>
          ))}
        </nav>
      </div>
    </header>
  );
}

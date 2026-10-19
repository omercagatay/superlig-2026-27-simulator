import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import {
  ApiError,
  getAccuracy,
  getForecast,
  getLiveData,
  getMatches,
  refreshLiveData,
  runSimulation,
  type AccuracyReport,
  type LiveData,
  type MatchesResponse,
  type SimResponse,
  type WhatIf,
} from "./api";
import { asOfLine, playedResults, prunePlayedPicks } from "./calendar";
import { describeError, type Failure } from "./errors";
import { useLang, useT } from "./i18n";
import { ClubDetail } from "./components/ClubDetail";
import { Footer, type Theme } from "./components/Footer";
import { Header, pageFromHash, type Page } from "./components/Header";
import { FixturesPage } from "./pages/FixturesPage";
import { OverviewPage } from "./pages/OverviewPage";
import { ProbabilitiesPage } from "./pages/ProbabilitiesPage";
import { ResultsPage } from "./pages/ResultsPage";
import { TablePage, type TableView } from "./pages/TablePage";

// Must match --bg in styles.css and the pre-paint script in index.html.
const THEME_COLOR: Record<Theme, string> = { light: "#ffffff", dark: "#16181c" };

// The settings GET /api/forecast answers for; the footer starts on them.
const DEFAULT_SIMS = 50000;
const DEFAULT_SEED = 12345;

// index.html makes the same choice before first paint; this only has to
// agree with it so React state matches the attribute already set.
function initialTheme(): Theme {
  try {
    const saved = localStorage.getItem("theme");
    if (saved === "light" || saved === "dark") return saved;
  } catch {
    /* site data blocked: fall back to the OS preference */
  }
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

/** The server's cached default forecast; an older backend without
 *  /api/forecast simulates the same thing on request. */
function defaultForecast(signal: AbortSignal): Promise<SimResponse> {
  return getForecast(signal).catch((e: unknown) => {
    if (e instanceof ApiError && e.status === 404) return runSimulation({}, signal);
    throw e;
  });
}

export default function App() {
  const t = useT();
  const { lang } = useLang();
  const [data, setData] = useState<SimResponse | null>(null);
  const [liveData, setLiveData] = useState<LiveData | null>(null);
  const [matchesData, setMatchesData] = useState<MatchesResponse | null>(null);
  const [matchesFailed, setMatchesFailed] = useState(false);
  const [accuracy, setAccuracy] = useState<AccuracyReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<Failure | null>(null);
  const [nSims, setNSims] = useState(DEFAULT_SIMS);
  const [seed, setSeed] = useState(DEFAULT_SEED);
  const [whatIf, setWhatIf] = useState<WhatIf[]>([]);
  const [prunedPicks, setPrunedPicks] = useState(0);
  const [theme, setTheme] = useState<Theme>(initialTheme);
  const [page, setPage] = useState<Page>(pageFromHash);
  // Null until the reader picks one; kept across page changes.
  const [tableView, setTableView] = useState<TableView | null>(null);
  const [club, setClub] = useState<string | null>(null);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    try {
      localStorage.setItem("theme", theme);
    } catch {
      /* site data blocked: the choice just isn't remembered */
    }
    document
      .querySelector('meta[name="theme-color"]')
      ?.setAttribute("content", THEME_COLOR[theme]);
  }, [theme]);

  useEffect(() => {
    const onHash = () => setPage(pageFromHash());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  // The picks and settings as they are now, for a refresh that finishes
  // renders later.
  const whatIfRef = useRef(whatIf);
  useEffect(() => {
    whatIfRef.current = whatIf;
  }, [whatIf]);
  const settingsRef = useRef({ nSims, seed });
  useEffect(() => {
    settingsRef.current = { nSims, seed };
  }, [nSims, seed]);

  const go = useCallback((p: Page) => {
    if (window.location.hash !== `#${p}`) window.location.hash = p;
    setPage(p);
    window.scrollTo({ top: 0 });
  }, []);

  const closeClub = useCallback(() => setClub(null), []);

  // Only the latest forecast request may land: each one aborts the one
  // before, and an answer that still arrives late is dropped.
  const latestRun = useRef<{ id: number; abort: AbortController | null }>({ id: 0, abort: null });

  /** Shows the forecast `load` fetches. Resolves to the failure, or null
   *  when it succeeded or a newer request took over. */
  const showForecast = useCallback(
    async (load: (signal: AbortSignal) => Promise<SimResponse>): Promise<unknown> => {
      const run = latestRun.current;
      run.abort?.abort();
      const abort = new AbortController();
      const id = ++run.id;
      run.abort = abort;
      setLoading(true);
      try {
        const result = await load(abort.signal);
        if (id === run.id) setData(result);
        return null;
      } catch (e) {
        return id === run.id ? e : null;
      } finally {
        if (id === run.id) setLoading(false);
      }
    },
    [],
  );

  /** Takes in a fresh calendar and drops the picks on fixtures it shows as
   *  played (the backend refuses those). Returns the picks still standing. */
  const applyMatches = useCallback((matches: MatchesResponse): WhatIf[] => {
    setMatchesData(matches);
    setMatchesFailed(false);
    const pins = whatIfRef.current;
    const kept = prunePlayedPicks(pins, matches);
    if (kept.length < pins.length) {
      whatIfRef.current = kept;
      setWhatIf(kept);
      // Added up: a refresh can prune, then its simulation's 400 recovery
      // prune again. Run and refresh reset the count when they start.
      setPrunedPicks((n) => n + pins.length - kept.length);
    }
    return kept;
  }, []);

  /** Shows a fresh simulation with these settings and picks. Resolves to
   *  the failure, or null when it succeeded or a newer request took over. */
  const simulatePicks = useCallback(
    async (n: number, s: number, pins: WhatIf[]): Promise<unknown> => {
      const simulate = (p: WhatIf[]) =>
        showForecast((signal) => runSimulation({ n_sims: n, seed: s, what_if: p }, signal));
      const failure = await simulate(pins);
      // The server can record a picked match after this page loaded its
      // calendar (its own timed refresh, or a refresh here whose calendar
      // reload failed), and then refuses the pick (400). Reload the
      // calendar; if that drops picks, ask once more without them.
      if (!(failure instanceof ApiError && failure.status === 400 && pins.length > 0)) return failure;
      const id = latestRun.current.id;
      const matches = await getMatches().catch(() => null);
      const before = whatIfRef.current.length;
      const kept = matches ? applyMatches(matches) : whatIfRef.current;
      // A request started meanwhile owns the page now, error line included.
      if (id !== latestRun.current.id) return null;
      return kept.length < before ? simulate(kept) : failure;
    },
    [applyMatches, showForecast],
  );

  const handleSimulate = useCallback(async () => {
    setError(null);
    setPrunedPicks(0);
    const failure = await simulatePicks(nSims, seed, whatIf);
    if (failure != null) setError({ error: failure, during: "forecast" });
  }, [simulatePicks, nSims, seed, whatIf]);

  // First load: hydrate cached live data (the backend refreshes it in the
  // background) and show the server's cached forecast without any clicks.
  const bootedRef = useRef(false);
  useEffect(() => {
    if (bootedRef.current) return;
    bootedRef.current = true;
    getLiveData()
      .then((live) => {
        if (live) setLiveData(live);
      })
      .catch(() => {
        /* cached live data is optional; refresh is still available */
      });
    getMatches()
      .then(applyMatches)
      // Fixtures and Results need the calendar; say it failed rather than
      // "loading" forever. "Update live data" fetches it again.
      .catch(() => setMatchesFailed(true));
    getAccuracy()
      .then(setAccuracy)
      .catch(() => {
        /* the report card is optional */
      });
    void showForecast(defaultForecast).then((failure) => {
      if (failure != null) setError({ error: failure, during: "forecast" });
    });
  }, [applyMatches, showForecast]);

  const handleRefresh = useCallback(async () => {
    setRefreshing(true);
    setError(null);
    setPrunedPicks(0);
    let failure: Failure | null = null;
    let refreshed = false;
    try {
      setLiveData(await refreshLiveData());
      refreshed = true;
    } catch (e) {
      failure = { error: e, during: "refresh" };
    }
    // Calendar and report land on their own, whatever else fails; fetching
    // them after a failed refresh still recovers a calendar that never loaded.
    const [matches, report] = await Promise.allSettled([getMatches(), getAccuracy()]);
    if (report.status === "fulfilled") setAccuracy(report.value);
    const before = whatIfRef.current;
    let pins = before;
    if (matches.status === "fulfilled") pins = applyMatches(matches.value);
    else setMatchesFailed(true);
    // The refresh moves the backend's played results and ratings on, and a
    // dropped pick changes the question; recompute so no view is stale.
    // The settings are read now, not at the click, so a Run pressed during
    // the refresh is not replaced by a forecast for the old ones.
    if (refreshed || pins.length < before.length) {
      const { nSims: n, seed: s } = settingsRef.current;
      const unchanged = n === DEFAULT_SIMS && s === DEFAULT_SEED && pins.length === 0;
      const simFailure = unchanged ? await showForecast(defaultForecast) : await simulatePicks(n, s, pins);
      if (failure == null && simFailure != null) failure = { error: simFailure, during: "forecast" };
    }
    setError(failure);
    setRefreshing(false);
  }, [applyMatches, showForecast, simulatePicks]);

  const selected = data?.teams.find((x) => x.team === club) ?? null;
  const shownError = error && describeError(error.error, error.during, t);
  const calendarNote = t(matchesFailed ? "calendarUnavailable" : "calendarLoading");
  const assumed = data?.what_if.length ?? 0;
  // The calendar carries every played result even before a live scrape is
  // cached; the scrape stands in when the calendar failed to load.
  const results = matchesData ? playedResults(matchesData) : (liveData?.played_matches ?? null);

  // What stands in for the simulation until it arrives, or when it failed.
  const noForecast = loading ? (
    <p className="muted">{t("simulating")}</p>
  ) : (
    <div>
      <p className="muted">{t("noForecast")}</p>
      <button type="button" className="btn btn-primary" onClick={() => void handleSimulate()}>
        {t("runSimulation")}
      </button>
    </div>
  );

  // Only the forecast pages wait on the simulation; Fixtures and Results
  // have data of their own and stay up when it fails, and so does Table's
  // current table, which comes with the calendar.
  let content: ReactNode;
  if (page === "fixtures") {
    content = matchesData ? (
      <FixturesPage
        data={matchesData}
        whatIf={whatIf}
        onWhatIf={setWhatIf}
        onRecalculate={() => void handleSimulate()}
        busy={loading}
      />
    ) : (
      <p className="muted">{calendarNote}</p>
    );
  } else if (page === "results") {
    content = results ? <ResultsPage played={results} /> : <p className="muted">{calendarNote}</p>;
  } else if (page === "table") {
    // The current table unless the reader chose, or there is nothing in it
    // yet (preseason) or the calendar failed.
    const view = tableView ?? (matchesFailed || matchesData?.as_of.played === 0 ? "projected" : "current");
    content = (
      <TablePage
        current={matchesData?.standings ?? null}
        projected={data?.table ?? null}
        view={view}
        onView={setTableView}
        currentMissing={<p className="muted">{calendarNote}</p>}
        projectedMissing={noForecast}
        onSelectClub={setClub}
      />
    );
  } else if (!data) {
    content = noForecast;
  } else if (page === "overview") {
    content = (
      <OverviewPage
        data={data}
        matches={matchesData}
        calendarNote={calendarNote}
        accuracy={accuracy}
        onOpenFixtures={() => go("fixtures")}
        onSelectClub={setClub}
      />
    );
  } else {
    content = <ProbabilitiesPage data={data} onSelectClub={setClub} />;
  }

  return (
    <>
      <Header page={page} onGo={go} />
      {assumed > 0 && (
        <p className="notice">
          {t("whatIfNotice")} ({assumed}).
        </p>
      )}
      {prunedPicks > 0 && (
        <p className="notice" role="status">
          {t("picksRemoved")} ({prunedPicks}).
        </p>
      )}
      {shownError && (
        <p className="notice notice-error" role="alert">
          <strong>{t("errorLabel")}:</strong> {shownError.message}
          {/* The backend's words, in English, for whoever reports the problem. */}
          {shownError.detail && (
            <>
              {" "}
              <span className="muted" lang="en">
                ({shownError.detail})
              </span>
            </>
          )}
        </p>
      )}
      <main>
        {/* What the forecasts stand on. Results says the same in its lead. */}
        {matchesData && page !== "results" && (
          <p className="as-of">
            {asOfLine(matchesData.as_of, lang, {
              none: t("asOfNone"),
              some: t("asOfSome"),
              all: t("asOfAll"),
            })}
          </p>
        )}
        {content}
      </main>
      <Footer
        nSims={nSims}
        seed={seed}
        onSims={setNSims}
        onSeed={setSeed}
        onRun={() => void handleSimulate()}
        onRefresh={() => void handleRefresh()}
        loading={loading}
        refreshing={refreshing}
        fetchedAt={liveData?.fetched_at ?? null}
        theme={theme}
        onTheme={setTheme}
      />
      {selected && data && (
        <ClubDetail
          team={selected}
          position={data.positions.find((p) => p.team === selected.team)}
          matches={matchesData}
          onClose={closeClub}
        />
      )}
    </>
  );
}

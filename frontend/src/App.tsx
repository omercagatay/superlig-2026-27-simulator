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
import { playedResults, prunePlayedPicks } from "./calendar";
import { useT, type Key } from "./i18n";
import { isBusy } from "./retry";
import { ClubDetail } from "./components/ClubDetail";
import { Footer, type Theme } from "./components/Footer";
import { Header, pageFromHash, type Page } from "./components/Header";
import { FixturesPage } from "./pages/FixturesPage";
import { OverviewPage } from "./pages/OverviewPage";
import { ProbabilitiesPage } from "./pages/ProbabilitiesPage";
import { ResultsPage } from "./pages/ResultsPage";
import { TablePage } from "./pages/TablePage";

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

/** A busy server gets a friendly line; any other failure the backend's own
 *  words. */
function errorText(e: unknown, t: (key: Key) => string): string {
  if (e instanceof ApiError && isBusy(e.status)) return t("serverBusy");
  return e instanceof Error ? e.message : String(e);
}

export default function App() {
  const t = useT();
  const [data, setData] = useState<SimResponse | null>(null);
  const [liveData, setLiveData] = useState<LiveData | null>(null);
  const [matchesData, setMatchesData] = useState<MatchesResponse | null>(null);
  const [matchesFailed, setMatchesFailed] = useState(false);
  const [accuracy, setAccuracy] = useState<AccuracyReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [nSims, setNSims] = useState(DEFAULT_SIMS);
  const [seed, setSeed] = useState(DEFAULT_SEED);
  const [whatIf, setWhatIf] = useState<WhatIf[]>([]);
  const [prunedPicks, setPrunedPicks] = useState(0);
  const [theme, setTheme] = useState<Theme>(initialTheme);
  const [page, setPage] = useState<Page>(pageFromHash);
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

  // The picks as they are now, for a refresh that finishes renders later.
  const whatIfRef = useRef(whatIf);
  useEffect(() => {
    whatIfRef.current = whatIf;
  }, [whatIf]);

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
      setPrunedPicks(pins.length - kept.length);
    }
    return kept;
  }, []);

  const handleSimulate = useCallback(async () => {
    setError(null);
    setPrunedPicks(0);
    const failure = await showForecast((signal) =>
      runSimulation({ n_sims: nSims, seed, what_if: whatIf }, signal),
    );
    if (failure != null) setError(failure);
  }, [showForecast, nSims, seed, whatIf]);

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
      if (failure != null) setError(failure);
    });
  }, [applyMatches, showForecast]);

  const handleRefresh = useCallback(async () => {
    setRefreshing(true);
    setError(null);
    setPrunedPicks(0);
    let failure: unknown = null;
    let refreshed = false;
    try {
      setLiveData(await refreshLiveData());
      refreshed = true;
    } catch (e) {
      failure = e;
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
    if (refreshed || pins.length < before.length) {
      const unchanged = nSims === DEFAULT_SIMS && seed === DEFAULT_SEED && pins.length === 0;
      const simFailure = await showForecast((signal) =>
        unchanged ? defaultForecast(signal) : runSimulation({ n_sims: nSims, seed, what_if: pins }, signal),
      );
      if (failure == null) failure = simFailure;
    }
    setError(failure);
    setRefreshing(false);
  }, [applyMatches, showForecast, nSims, seed]);

  const selected = data?.teams.find((x) => x.team === club) ?? null;
  const calendarNote = t(matchesFailed ? "calendarUnavailable" : "calendarLoading");
  const assumed = data?.what_if.length ?? 0;
  // The calendar carries every played result even before a live scrape is
  // cached; the scrape stands in when the calendar failed to load.
  const results = matchesData ? playedResults(matchesData) : (liveData?.played_matches ?? null);

  // Only the forecast pages wait on the simulation; Fixtures and Results
  // have data of their own and stay up when it fails.
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
  } else if (!data) {
    content = loading ? (
      <p className="muted">{t("simulating")}</p>
    ) : (
      <div>
        <p className="muted">{t("noForecast")}</p>
        <button type="button" className="btn btn-primary" onClick={() => void handleSimulate()}>
          {t("runSimulation")}
        </button>
      </div>
    );
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
  } else if (page === "table") {
    content = <TablePage table={data.table} onSelectClub={setClub} />;
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
      {error != null && (
        <p className="notice notice-error" role="alert">
          <strong>{t("errorLabel")}:</strong> {errorText(error, t)}
        </p>
      )}
      <main>{content}</main>
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

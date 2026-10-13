import { useCallback, useEffect, useRef, useState } from "react";
import {
  getAccuracy,
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
import { useT } from "./i18n";
import { ClubDetail } from "./components/ClubDetail";
import { Colophon, type Theme } from "./components/Colophon";
import { Masthead, pageFromHash, type Page } from "./components/Masthead";
import { CoverPage } from "./pages/CoverPage";
import { FixturesPage } from "./pages/FixturesPage";
import { RacesPage } from "./pages/RacesPage";
import { ResultsPage } from "./pages/ResultsPage";
import { TablePage } from "./pages/TablePage";

// index.html makes the same choice before first paint; this only has to
// agree with it so React state matches the attribute already set.
function initialTheme(): Theme {
  const saved = localStorage.getItem("theme");
  if (saved === "light" || saved === "dark") return saved;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
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
  const [error, setError] = useState<string | null>(null);
  const [nSims, setNSims] = useState(50000);
  const [seed, setSeed] = useState(12345);
  const [whatIf, setWhatIf] = useState<WhatIf[]>([]);
  const [theme, setTheme] = useState<Theme>(initialTheme);
  const [page, setPage] = useState<Page>(pageFromHash);
  const [club, setClub] = useState<string | null>(null);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem("theme", theme);
    document
      .querySelector('meta[name="theme-color"]')
      ?.setAttribute("content", theme === "dark" ? "#0f1724" : "#f2ecdf");
  }, [theme]);

  useEffect(() => {
    const onHash = () => setPage(pageFromHash());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  const go = useCallback((p: Page) => {
    if (window.location.hash !== `#${p}`) window.location.hash = p;
    setPage(p);
    window.scrollTo({ top: 0 });
  }, []);

  const closeClub = useCallback(() => setClub(null), []);

  const handleSimulate = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setData(await runSimulation({ n_sims: nSims, seed, what_if: whatIf }));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [nSims, seed, whatIf]);

  // First load: hydrate cached live data (the backend refreshes it in the
  // background) and print a first edition without any clicks.
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
      .then(setMatchesData)
      // The calendar is optional on every page but "Bu Hafta"; say it failed
      // rather than "loading" forever. "Update live data" fetches it again.
      .catch(() => setMatchesFailed(true));
    getAccuracy()
      .then(setAccuracy)
      .catch(() => {
        /* the report card is optional */
      });
    void handleSimulate();
  }, [handleSimulate]);

  const handleRefresh = useCallback(async () => {
    setRefreshing(true);
    setError(null);
    try {
      setLiveData(await refreshLiveData());
      // The refresh moves the backend's played results and ratings on;
      // reprint every derived page so none keeps the old edition.
      const [result, matches, report] = await Promise.all([
        runSimulation({ n_sims: nSims, seed, what_if: whatIf }),
        getMatches().catch(() => null),
        getAccuracy().catch(() => null),
      ]);
      setData(result);
      if (matches) setMatchesData(matches);
      if (report) setAccuracy(report);
    } catch (e) {
      setError(String(e));
    } finally {
      setRefreshing(false);
    }
  }, [nSims, seed, whatIf]);

  const issue = matchesData?.current_round ?? null;
  const issueDates =
    matchesData?.rounds.find((r) => r.round === issue)?.matches.map((m) => m.date) ?? [];
  const selected = data?.teams.find((x) => x.team === club) ?? null;
  const calendarNote = t(matchesFailed ? "calendarUnavailable" : "calendarLoading");

  return (
    <div className="sheet">
      <Masthead
        issue={issue}
        dates={issueDates}
        conditional={(data?.what_if.length ?? 0) > 0}
        page={page}
        onGo={go}
      />
      {error && (
        <p className="erratum" role="alert">
          <strong>{t("pressError")}:</strong> {error}
        </p>
      )}
      <main>
        {!data ? (
          loading ? (
            <p className="in-press">
              {t("inPress")}
              <small>{t("inPressBody")}</small>
            </p>
          ) : (
            <div className="in-press">
              {t("noForecast")}
              <small>{t("noForecastBody")}</small>
              <p>
                <button type="button" className="btn btn-red" onClick={() => void handleSimulate()}>
                  {t("runSimulation")}
                </button>
              </p>
            </div>
          )
        ) : (
          <>
            {page === "cover" && (
              <CoverPage
                data={data}
                matches={matchesData}
                calendarNote={calendarNote}
                accuracy={accuracy}
                onOpenFixtures={() => go("fixtures")}
                onSelectClub={setClub}
              />
            )}
            {page === "fixtures" &&
              (matchesData ? (
                <FixturesPage
                  data={matchesData}
                  whatIf={whatIf}
                  onWhatIf={setWhatIf}
                  onReprint={() => void handleSimulate()}
                  busy={loading}
                />
              ) : (
                <p className="note">{calendarNote}</p>
              ))}
            {page === "table" && <TablePage table={data.table} onSelectClub={setClub} />}
            {page === "races" && <RacesPage data={data} onSelectClub={setClub} />}
            {page === "results" &&
              (liveData ? <ResultsPage liveData={liveData} /> : <p className="note">{t("noLiveData")}</p>)}
          </>
        )}
      </main>
      <Colophon
        nSims={nSims}
        seed={seed}
        onSims={setNSims}
        onSeed={setSeed}
        onReprint={() => void handleSimulate()}
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
    </div>
  );
}

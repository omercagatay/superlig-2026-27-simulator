import { useState, useCallback, useEffect, useRef } from "react";
import {
  runSimulation,
  runScenario,
  refreshLiveData,
  getLiveData,
  getUpcoming,
  getMatches,
  type SimResponse,
  type LiveData,
  type UpcomingMatch,
  type MatchesResponse,
} from "./api";
import { ForecastView } from "./components/ForecastView";
import { LeagueTable } from "./components/LeagueTable";
import { PositionGrid } from "./components/PositionGrid";
import { LiveStats } from "./components/LiveStats";
import { MatchesView } from "./components/MatchesView";

type DashboardView = "forecast" | "positions" | "table" | "matches" | "live";

type Theme = "dark" | "light";

// index.html applies the same resolution before first paint; this only
// needs to agree with it so React state matches the pre-set attribute.
function initialTheme(): Theme {
  const saved = localStorage.getItem("theme");
  if (saved === "light" || saved === "dark") return saved;
  return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

const sunIcon = (
  <svg
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    aria-hidden="true"
  >
    <circle cx="12" cy="12" r="4.2" />
    <path d="M12 2.5v2.6M12 18.9v2.6M2.5 12h2.6M18.9 12h2.6M5.2 5.2l1.9 1.9M16.9 16.9l1.9 1.9M18.8 5.2l-1.9 1.9M7.1 16.9l-1.9 1.9" />
  </svg>
);

const moonIcon = (
  <svg
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
  >
    <path d="M20.4 14.2A8.5 8.5 0 0 1 9.8 3.6a8.5 8.5 0 1 0 10.6 10.6Z" />
  </svg>
);

export default function App() {
  const [data, setData] = useState<SimResponse | null>(null);
  const [liveData, setLiveData] = useState<LiveData | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [nSims, setNSims] = useState(50000);
  const [seed, setSeed] = useState(12345);
  const [upcoming, setUpcoming] = useState<UpcomingMatch[]>([]);
  const [matchesData, setMatchesData] = useState<MatchesResponse | null>(null);
  const [activeView, setActiveView] = useState<DashboardView>("forecast");
  const [theme, setTheme] = useState<Theme>(initialTheme);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem("theme", theme);
    document
      .querySelector('meta[name="theme-color"]')
      ?.setAttribute("content", theme === "light" ? "#e9ede7" : "#0b0e0c");
  }, [theme]);

  const handleSimulate = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await runSimulation({ n_sims: nSims, seed });
      setData(result);
      setActiveView("forecast");
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [nSims, seed]);

  const handleScenario = useCallback(
    async (prompt: string) => {
      setLoading(true);
      setError(null);
      try {
        const result = await runScenario({ prompt, n_sims: nSims, seed });
        setData(result);
        setActiveView("forecast");
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    },
    [nSims, seed]
  );

  // On first load: hydrate cached live data (the backend refreshes it in
  // the background) and kick off an initial forecast so the dashboard is
  // populated without any clicks.
  const bootedRef = useRef(false);
  useEffect(() => {
    if (bootedRef.current) return;
    bootedRef.current = true;
    getLiveData()
      .then((live) => {
        if (live) setLiveData(live);
      })
      .catch(() => {
        /* cached live data is optional; manual refresh still available */
      });
    getUpcoming()
      .then((u) => setUpcoming(u.matches))
      .catch(() => {
        /* upcoming forecasts are optional decoration */
      });
    getMatches()
      .then(setMatchesData)
      .catch(() => {
        /* per-match forecasts are optional decoration */
      });
    void handleSimulate();
  }, [handleSimulate]);

  const handleRefresh = useCallback(async () => {
    setRefreshing(true);
    setError(null);
    try {
      const live = await refreshLiveData();
      setLiveData(live);
      if (!data) setActiveView("live");
    } catch (e) {
      setError(String(e));
    } finally {
      setRefreshing(false);
    }
  }, [data]);

  const liveMatchCount = liveData ? liveData.played_matches.length : 0;
  const lastUpdated = (() => {
    // The scraper stamps RFC 3339 (chrono's to_rfc3339).
    const t = Date.parse(liveData?.fetched_at ?? "");
    return Number.isFinite(t)
      ? new Date(t).toLocaleString(undefined, {
          month: "short",
          day: "numeric",
          hour: "2-digit",
          minute: "2-digit",
        })
      : null;
  })();

  const tabs: { id: DashboardView; label: string; disabled: boolean; count?: number }[] = [
    { id: "forecast", label: "Forecast", disabled: !data },
    { id: "positions", label: "Positions", disabled: !data },
    { id: "table", label: "Table", disabled: !data },
    { id: "matches", label: "Matches", disabled: !matchesData },
    { id: "live", label: "Live", disabled: !liveData, count: liveData ? liveMatchCount : undefined },
  ];

  return (
    <div className="app">
      <header className="topbar">
        <div className="topbar-inner">
          <div className="brand">
            <span className="brand-mark">SL</span>
            <div>
              <h1>Süper Lig Forecast</h1>
              <span className="brand-sub">2026-27 Monte Carlo season simulator</span>
            </div>
          </div>
          <div className="topbar-status">
            {data && <span>{data.n_sims.toLocaleString()} simulations</span>}
            {lastUpdated && (
              <span>
                <span className="live-dot" aria-hidden="true" />
                updated {lastUpdated}
              </span>
            )}
            {data?.scenario_applied && <span className="badge-scenario">Scenario</span>}
          </div>
          <form
            className="run-controls"
            onSubmit={(e) => {
              e.preventDefault();
              void handleSimulate();
            }}
          >
            <label>
              Sims
              <input
                type="number"
                value={nSims}
                onChange={(e) => setNSims(Number(e.target.value))}
                min={100}
                max={200000}
                step={1000}
              />
            </label>
            <label>
              Seed
              <input type="number" value={seed} onChange={(e) => setSeed(Number(e.target.value))} />
            </label>
            <button type="submit" className="btn btn-primary" disabled={loading}>
              {loading ? "Running…" : "Run"}
            </button>
            <button type="button" className="btn" onClick={handleRefresh} disabled={refreshing}>
              {refreshing ? "Updating…" : "Update live data"}
            </button>
          </form>
          <button
            type="button"
            className="theme-toggle"
            onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
            aria-label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
            title={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
          >
            {theme === "dark" ? sunIcon : moonIcon}
          </button>
        </div>
        <nav className="tabs" role="tablist" aria-label="Dashboard views">
          {tabs.map((t) => (
            <button
              key={t.id}
              type="button"
              role="tab"
              className="tab"
              aria-selected={activeView === t.id}
              disabled={t.disabled}
              onClick={() => setActiveView(t.id)}
            >
              {t.label}
              {t.count != null && <span className="tab-count">{t.count}</span>}
            </button>
          ))}
        </nav>
      </header>

      {error && (
        <div className="error-banner" role="alert">
          {error}
        </div>
      )}

      <main className="content">
        {!data && loading && (
          <div className="boot-state">
            <div className="boot-spinner" aria-hidden="true" />
            <span className="eyebrow">Simulating</span>
            <p>{nSims.toLocaleString()} seasons, in parallel. A few seconds.</p>
          </div>
        )}

        {!data && !loading && (
          <div className="boot-state">
            <span className="eyebrow">No forecast yet</span>
            <p style={{ marginBottom: "1rem" }}>
              Run the simulation to see how the 2026-27 season plays out.
            </p>
            <button className="btn btn-primary" onClick={handleSimulate}>
              Run simulation
            </button>
          </div>
        )}

        {data && activeView === "forecast" && (
          <ForecastView
            data={data}
            liveData={liveData}
            upcoming={upcoming}
            loading={loading}
            onScenario={handleScenario}
            liveMatchCount={liveMatchCount}
            onShowLive={() => setActiveView("live")}
          />
        )}

        {data && activeView === "positions" && <PositionGrid positions={data.positions} />}

        {data && activeView === "table" && <LeagueTable table={data.table} />}

        {matchesData && activeView === "matches" && <MatchesView data={matchesData} />}

        {liveData && activeView === "live" && <LiveStats liveData={liveData} />}
      </main>
    </div>
  );
}

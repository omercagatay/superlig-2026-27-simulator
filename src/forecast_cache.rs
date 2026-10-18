//! Season simulations cached per world state.
//!
//! A default 50,000-season run occupies every core for seconds, and its
//! answer is the same for every visitor until a live refresh changes the
//! `World`. Results are cached under the world's version, and concurrent
//! identical requests share one computation instead of each queueing its own.

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::OnceCell;

use crate::sim::ForcedOutcome;

/// Season count and seed of the default forecast, served by both
/// `GET /api/forecast` and `POST /api/simulate {}`.
pub const DEFAULT_N_SIMS: usize = 50_000;
pub const DEFAULT_SEED: u64 = 12_345;

/// A validated simulation request in canonical form: the same overrides and
/// pins in any order give the same key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SimKey {
    n_sims: usize,
    seed: u64,
    /// `(club, rating bits)`, sorted by club. Ratings are validated finite
    /// and in range before a key is built, so bit equality is value equality.
    elo_overrides: Vec<(String, u64)>,
    /// `((home, away), outcome)`, sorted by fixture.
    forced: Vec<((usize, usize), u8)>,
}

impl SimKey {
    pub fn new(
        n_sims: usize,
        seed: u64,
        elo_overrides: &HashMap<String, f64>,
        forced: &HashMap<(usize, usize), ForcedOutcome>,
    ) -> Self {
        let mut elo_overrides: Vec<(String, u64)> = elo_overrides
            .iter()
            .map(|(club, rating)| (club.clone(), rating.to_bits()))
            .collect();
        elo_overrides.sort_unstable();
        let mut forced: Vec<((usize, usize), u8)> = forced
            .iter()
            .map(|(&fixture, &outcome)| {
                let code = match outcome {
                    ForcedOutcome::Home => 0,
                    ForcedOutcome::Draw => 1,
                    ForcedOutcome::Away => 2,
                };
                (fixture, code)
            })
            .collect();
        forced.sort_unstable();
        Self {
            n_sims,
            seed,
            elo_overrides,
            forced,
        }
    }

    /// The key of the default forecast: no overrides, no pins.
    pub fn default_forecast() -> Self {
        Self::new(
            DEFAULT_N_SIMS,
            DEFAULT_SEED,
            &HashMap::new(),
            &HashMap::new(),
        )
    }
}

/// A small bounded cache of values computed against the current world
/// version, with single-flight computation: a caller asking for a key that is
/// already being computed waits for that result instead of starting its own.
pub struct ResponseCache<K, V, E> {
    capacity: usize,
    /// Never evicted for space (the default forecast); a version change still
    /// drops it.
    pinned: Option<K>,
    state: Mutex<CacheState<K, V, E>>,
}

struct CacheState<K, V, E> {
    /// Newest world version asked for; only its entries are kept.
    version: u64,
    entries: HashMap<K, Entry<V, E>>,
    /// Access counter for least-recently-used eviction.
    clock: u64,
}

/// One computation's outcome, shared by everyone who waited for it.
type Outcome<V, E> = Arc<OnceCell<Result<V, E>>>;

struct Entry<V, E> {
    cell: Outcome<V, E>,
    last_used: u64,
}

impl<K: Clone + Eq + Hash, V: Clone, E: Clone> ResponseCache<K, V, E> {
    pub fn new(capacity: usize, pinned: Option<K>) -> Self {
        Self {
            capacity: capacity.max(1),
            pinned,
            state: Mutex::new(CacheState {
                version: 0,
                entries: HashMap::new(),
                clock: 0,
            }),
        }
    }

    /// The finished value for `key` at `version`, if there is one.
    pub fn get(&self, version: u64, key: &K) -> Option<V> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.version != version {
            return None;
        }
        state.clock += 1;
        let now = state.clock;
        let entry = state.entries.get_mut(key)?;
        entry.last_used = now;
        match entry.cell.get() {
            Some(Ok(value)) => Some(value.clone()),
            _ => None,
        }
    }

    /// The shared cell for `key` at `version`, created if needed. A newer
    /// version drops every older entry. A caller still on an older version
    /// gets a private cell: its request was validated against a world that
    /// has since changed, so it may compute an answer but must not publish it.
    fn cell(&self, version: u64, key: K) -> Outcome<V, E> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if version < state.version {
            return Arc::new(OnceCell::new());
        }
        if version > state.version {
            state.version = version;
            state.entries.clear();
        }
        state.clock += 1;
        let now = state.clock;
        if let Some(entry) = state.entries.get_mut(&key) {
            entry.last_used = now;
            // A failure is not cached: a caller arriving after it starts
            // afresh.
            if matches!(entry.cell.get(), Some(Err(_))) {
                entry.cell = Arc::new(OnceCell::new());
            }
            return entry.cell.clone();
        }
        if state.entries.len() >= self.capacity {
            let oldest = state
                .entries
                .iter()
                .filter(|(k, _)| self.pinned.as_ref() != Some(*k))
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(k, _)| k.clone());
            if let Some(k) = oldest {
                state.entries.remove(&k);
            }
        }
        let cell = Arc::new(OnceCell::new());
        state.entries.insert(
            key,
            Entry {
                cell: cell.clone(),
                last_used: now,
            },
        );
        cell
    }

    /// The value for `key` at `version`: the cached result, the outcome of a
    /// computation already in flight, or a fresh run of `compute`. Everyone
    /// waiting on a run that fails gets its error, so no caller waits through
    /// a chain of retries; the next caller to arrive afterwards retries.
    ///
    /// The computation runs in its own task, so a caller that disconnects
    /// mid-run does not cancel the result everyone else is waiting for.
    pub async fn get_or_compute<F, Fut>(&self, version: u64, key: K, compute: F) -> Result<V, E>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<V, E>> + Send + 'static,
        V: Send + Sync + 'static,
        E: From<tokio::task::JoinError> + Send + Sync + 'static,
    {
        let cell = self.cell(version, key);
        if let Some(outcome) = cell.get() {
            return outcome.clone();
        }
        tokio::spawn(async move { cell.get_or_init(compute).await.clone() }).await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[derive(Clone, Debug, PartialEq)]
    enum TestError {
        Failed,
        Join,
    }

    impl From<tokio::task::JoinError> for TestError {
        fn from(_: tokio::task::JoinError) -> Self {
            TestError::Join
        }
    }

    type TestCache = ResponseCache<&'static str, u32, TestError>;

    async fn ready(cache: &TestCache, version: u64, value: u32) -> u32 {
        cache
            .get_or_compute(
                version,
                "k",
                move || async move { Ok::<_, TestError>(value) },
            )
            .await
            .unwrap()
    }

    #[test]
    fn sim_key_ignores_override_and_pin_order() {
        let clubs: Vec<String> = crate::data::clubs().iter().map(|c| c.to_string()).collect();
        let mut forward = HashMap::new();
        let mut backward = HashMap::new();
        for (i, club) in clubs.iter().enumerate() {
            forward.insert(club.clone(), 1500.0 + i as f64);
        }
        for (i, club) in clubs.iter().enumerate().rev() {
            backward.insert(club.clone(), 1500.0 + i as f64);
        }
        let outcomes = [
            ForcedOutcome::Home,
            ForcedOutcome::Draw,
            ForcedOutcome::Away,
        ];
        let mut pins_a = HashMap::new();
        let mut pins_b = HashMap::new();
        for i in 0..15 {
            pins_a.insert((i, i + 1), outcomes[i % 3]);
        }
        for i in (0..15).rev() {
            pins_b.insert((i, i + 1), outcomes[i % 3]);
        }
        assert_eq!(
            SimKey::new(1000, 7, &forward, &pins_a),
            SimKey::new(1000, 7, &backward, &pins_b)
        );

        let mut moved = forward.clone();
        *moved.get_mut(&clubs[0]).unwrap() += 1.0;
        assert_ne!(
            SimKey::new(1000, 7, &forward, &pins_a),
            SimKey::new(1000, 7, &moved, &pins_a)
        );
        let mut flipped = pins_a.clone();
        flipped.insert((0, 1), ForcedOutcome::Away);
        assert_ne!(
            SimKey::new(1000, 7, &forward, &pins_a),
            SimKey::new(1000, 7, &forward, &flipped)
        );
        assert_ne!(
            SimKey::new(1000, 7, &forward, &pins_a),
            SimKey::new(1000, 8, &forward, &pins_a)
        );
    }

    #[test]
    fn default_forecast_key_matches_an_empty_request() {
        assert_eq!(
            SimKey::default_forecast(),
            SimKey::new(50_000, 12_345, &HashMap::new(), &HashMap::new())
        );
    }

    #[tokio::test]
    async fn version_bump_invalidates_older_entries() {
        let cache = TestCache::new(4, None);
        assert_eq!(ready(&cache, 1, 10).await, 10);
        assert_eq!(cache.get(1, &"k"), Some(10));
        assert_eq!(
            cache.get(2, &"k"),
            None,
            "a new version must not see old results"
        );

        assert_eq!(ready(&cache, 2, 20).await, 20);
        assert_eq!(cache.get(2, &"k"), Some(20));
        assert_eq!(cache.get(1, &"k"), None);

        // A straggler validated against version 1 still gets an answer, but
        // it is neither served from nor stored in the version-2 cache.
        assert_eq!(ready(&cache, 1, 99).await, 99);
        assert_eq!(cache.get(2, &"k"), Some(20));
        assert_eq!(
            ready(&cache, 2, 0).await,
            20,
            "cached value, not a recompute"
        );
    }

    #[tokio::test]
    async fn concurrent_identical_requests_share_one_computation() {
        let cache = TestCache::new(4, None);
        let runs = Arc::new(AtomicUsize::new(0));
        let call = || {
            let runs = runs.clone();
            cache.get_or_compute(1, "k", move || async move {
                runs.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok::<_, TestError>(42u32)
            })
        };
        let (a, b, c) = tokio::join!(call(), call(), call());
        assert_eq!((a, b, c), (Ok(42), Ok(42), Ok(42)));
        assert_eq!(runs.load(Ordering::SeqCst), 1);

        assert_eq!(call().await, Ok(42));
        assert_eq!(runs.load(Ordering::SeqCst), 1, "a hit must not recompute");
    }

    #[tokio::test]
    async fn a_failed_computation_is_retried_by_the_next_caller() {
        let cache = TestCache::new(4, None);
        let failed = cache
            .get_or_compute(1, "k", || async { Err(TestError::Failed) })
            .await;
        assert_eq!(failed, Err(TestError::Failed));
        assert_eq!(cache.get(1, &"k"), None);
        assert_eq!(ready(&cache, 1, 5).await, 5);
        assert_eq!(cache.get(1, &"k"), Some(5));
    }

    #[tokio::test]
    async fn callers_waiting_on_a_failed_run_share_its_error_instead_of_retrying() {
        let cache = TestCache::new(4, None);
        let runs = Arc::new(AtomicUsize::new(0));
        let call = || {
            let runs = runs.clone();
            cache.get_or_compute(1, "k", move || async move {
                runs.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(50)).await;
                Err(TestError::Failed)
            })
        };
        let (a, b, c) = tokio::join!(call(), call(), call());
        assert_eq!(
            (a, b, c),
            (
                Err(TestError::Failed),
                Err(TestError::Failed),
                Err(TestError::Failed)
            )
        );
        assert_eq!(runs.load(Ordering::SeqCst), 1);

        assert_eq!(call().await, Err(TestError::Failed));
        assert_eq!(runs.load(Ordering::SeqCst), 2, "a later caller retries");
    }

    #[tokio::test]
    async fn eviction_drops_the_least_recently_used_but_never_the_pinned_key() {
        let cache = TestCache::new(2, Some("pinned"));
        let put =
            |key: &'static str| cache.get_or_compute(1, key, || async { Ok::<_, TestError>(1u32) });
        put("pinned").await.unwrap();
        put("a").await.unwrap();
        put("b").await.unwrap();
        assert_eq!(cache.get(1, &"pinned"), Some(1));
        assert_eq!(cache.get(1, &"a"), None, "least recently used goes first");
        assert_eq!(cache.get(1, &"b"), Some(1));
    }
}

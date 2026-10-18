//! Season simulations cached per world state.
//!
//! A default 50,000-season run occupies every core for seconds, and its
//! answer is the same for every visitor until a live refresh changes the
//! `World`. Results are cached under the world's version, and concurrent
//! identical requests share one computation instead of each queueing its own.

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::{watch, OnceCell};

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

struct Entry<V, E> {
    flight: Arc<Flight<V, E>>,
    last_used: u64,
}

/// One computation's outcome, shared by everyone who waited for it.
struct Flight<V, E> {
    outcome: OnceCell<Result<V, E>>,
    /// How many callers are waiting for `outcome` right now.
    interest: watch::Sender<usize>,
    /// Set once the run is admitted; from then on it finishes and publishes
    /// even if every caller leaves.
    admitted: AtomicBool,
}

/// A run given up before admission because nobody was waiting for it.
struct Abandoned;

impl<V, E> Flight<V, E> {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            outcome: OnceCell::new(),
            interest: watch::Sender::new(0),
            admitted: AtomicBool::new(false),
        })
    }

    /// Nobody is waiting and the run was never admitted: it is worth nothing.
    fn abandoned(&self) -> bool {
        !self.admitted.load(Ordering::Acquire) && *self.interest.borrow() == 0
    }

    /// Resolves once no caller is waiting any more.
    async fn deserted(&self) {
        // The flight owns the sender, so the channel never closes under us.
        let _ = self.interest.subscribe().wait_for(|&n| n == 0).await;
    }

    /// Wait for admission, then compute. While still waiting, give up as
    /// soon as nobody wants the answer, which frees whatever place `admit`
    /// was holding.
    async fn run<A, AFut, T, F, Fut>(&self, admit: A, compute: F) -> Result<Result<V, E>, Abandoned>
    where
        A: FnOnce() -> AFut,
        AFut: Future<Output = Result<T, E>>,
        F: FnOnce(T) -> Fut,
        Fut: Future<Output = Result<V, E>>,
    {
        let ticket = tokio::select! {
            biased;
            () = self.deserted() => return Err(Abandoned),
            ticket = admit() => ticket,
        };
        match ticket {
            Ok(ticket) => {
                self.admitted.store(true, Ordering::Release);
                Ok(compute(ticket).await)
            }
            Err(e) => Ok(Err(e)),
        }
    }
}

/// One caller's wait on a flight. Dropping it, when the caller gets its
/// answer or its future is dropped, withdraws that caller's interest.
struct Interest<V, E>(Arc<Flight<V, E>>);

impl<V, E> Interest<V, E> {
    fn new(flight: Arc<Flight<V, E>>) -> Self {
        flight.interest.send_modify(|n| *n += 1);
        Self(flight)
    }
}

impl<V, E> Drop for Interest<V, E> {
    fn drop(&mut self) {
        self.0.interest.send_modify(|n| *n -= 1);
    }
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
        match entry.flight.outcome.get() {
            Some(Ok(value)) => Some(value.clone()),
            _ => None,
        }
    }

    /// Wait on the shared flight for `key` at `version`, created if needed.
    /// The new interest is counted under the lock, so eviction never takes a
    /// flight just handed out for an abandoned one. A newer version drops
    /// every older entry. A caller still on an older version gets a private
    /// flight: its request was validated against a world that has since
    /// changed, so it may compute an answer but must not publish it.
    fn join(&self, version: u64, key: K) -> Interest<V, E> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if version < state.version {
            return Interest::new(Flight::new());
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
            // afresh. An abandoned flight is rejoined as it is: its run either
            // sees the new interest and carries on, or gives up and leaves the
            // cell to this caller's run.
            if matches!(entry.flight.outcome.get(), Some(Err(_))) {
                entry.flight = Flight::new();
            }
            return Interest::new(entry.flight.clone());
        }
        if state.entries.len() >= self.capacity {
            // A failed or abandoned run is worth nothing, so it goes first;
            // then finished answers, least recently used first. A run still
            // in flight goes last: dropping it would let an identical request
            // start a second computation, and a burst of 503s must not push
            // good answers out.
            let victim = state
                .entries
                .iter()
                .filter(|(k, _)| self.pinned.as_ref() != Some(*k))
                .min_by_key(|(_, entry)| {
                    let rank = match entry.flight.outcome.get() {
                        Some(Err(_)) => 0,
                        None if entry.flight.abandoned() => 0,
                        Some(Ok(_)) => 1,
                        None => 2,
                    };
                    (rank, entry.last_used)
                })
                .map(|(k, _)| k.clone());
            if let Some(k) = victim {
                state.entries.remove(&k);
            }
        }
        let flight = Flight::new();
        state.entries.insert(
            key,
            Entry {
                flight: flight.clone(),
                last_used: now,
            },
        );
        Interest::new(flight)
    }

    /// The value for `key` at `version`: the cached result, the outcome of a
    /// computation already in flight, or a fresh run. Everyone waiting on a
    /// run that fails gets its error, so no caller waits through a chain of
    /// retries; the next caller to arrive afterwards retries.
    ///
    /// A fresh run first waits in `admit` (for a simulation slot), then
    /// hands what it got to `compute`. It runs in its own task, so a caller
    /// that disconnects does not cancel the result everyone else is waiting
    /// for. But a run nobody waits for any more gives up while still in
    /// `admit`, instead of keeping its place there for work no one would
    /// receive; the next caller for the key starts afresh. Once admitted, a
    /// run always finishes and publishes.
    pub async fn get_or_compute<A, AFut, T, F, Fut>(
        &self,
        version: u64,
        key: K,
        admit: A,
        compute: F,
    ) -> Result<V, E>
    where
        A: FnOnce() -> AFut + Send + 'static,
        AFut: Future<Output = Result<T, E>> + Send + 'static,
        T: Send + 'static,
        F: FnOnce(T) -> Fut + Send + 'static,
        Fut: Future<Output = Result<V, E>> + Send + 'static,
        V: Send + Sync + 'static,
        E: From<tokio::task::JoinError> + Send + Sync + 'static,
    {
        let interest = self.join(version, key);
        if let Some(outcome) = interest.0.outcome.get() {
            return outcome.clone();
        }
        let flight = interest.0.clone();
        let outcome = tokio::spawn(async move {
            let run = || flight.run(admit, compute);
            flight.outcome.get_or_try_init(run).await.ok().cloned()
        })
        .await?;
        // Only a run nobody waits for is abandoned, and this caller still
        // holds its interest.
        outcome.expect("a run with a waiting caller is never abandoned")
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

    /// Admission for a run that needs no slot.
    async fn no_wait() -> Result<(), TestError> {
        Ok(())
    }

    async fn ready(cache: &TestCache, version: u64, value: u32) -> u32 {
        cache
            .get_or_compute(version, "k", no_wait, move |()| async move {
                Ok::<_, TestError>(value)
            })
            .await
            .unwrap()
    }

    type BoxFuture<T> = std::pin::Pin<Box<dyn Future<Output = T> + Send>>;

    /// A one-slot stand-in for the simulation queue: `admit` waits for the
    /// slot, and `waiting` counts the runs queued for it.
    #[derive(Clone)]
    struct Queue {
        slot: Arc<tokio::sync::Semaphore>,
        waiting: Arc<AtomicUsize>,
    }

    /// A place in the stand-in queue, given back however the wait ends.
    struct Seat(Arc<AtomicUsize>);

    impl Drop for Seat {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    impl Queue {
        /// A queue whose slot is taken until `open`.
        fn busy() -> Self {
            Self {
                slot: Arc::new(tokio::sync::Semaphore::new(0)),
                waiting: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn open(&self) {
            self.slot.add_permits(1);
        }

        fn waiting(&self) -> usize {
            self.waiting.load(Ordering::SeqCst)
        }

        fn admit(
            &self,
        ) -> impl FnOnce() -> BoxFuture<Result<tokio::sync::OwnedSemaphorePermit, TestError>>
        {
            let queue = self.clone();
            move || {
                Box::pin(async move {
                    queue.waiting.fetch_add(1, Ordering::SeqCst);
                    let _seat = Seat(queue.waiting.clone());
                    Ok(queue.slot.acquire_owned().await.expect("open semaphore"))
                })
            }
        }
    }

    /// A computation that counts its runs and answers `value`.
    fn counted<T>(
        runs: &Arc<AtomicUsize>,
        value: u32,
    ) -> impl FnOnce(T) -> BoxFuture<Result<u32, TestError>> {
        let runs = runs.clone();
        move |_slot| {
            runs.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok(value) })
        }
    }

    /// Wait until `done` holds, failing the test after a second.
    async fn until(done: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while !done() {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("the condition never held");
    }

    /// Callers currently waiting on the flight for `key`.
    fn interest(cache: &TestCache, key: &str) -> usize {
        *cache.state.lock().unwrap().entries[key]
            .flight
            .interest
            .borrow()
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
            cache.get_or_compute(1, "k", no_wait, move |()| async move {
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
            .get_or_compute(1, "k", no_wait, |()| async { Err(TestError::Failed) })
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
            cache.get_or_compute(1, "k", no_wait, move |()| async move {
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
        let put = |key: &'static str| {
            cache.get_or_compute(1, key, no_wait, |()| async { Ok::<_, TestError>(1u32) })
        };
        put("pinned").await.unwrap();
        put("a").await.unwrap();
        put("b").await.unwrap();
        assert_eq!(cache.get(1, &"pinned"), Some(1));
        assert_eq!(cache.get(1, &"a"), None, "least recently used goes first");
        assert_eq!(cache.get(1, &"b"), Some(1));
    }

    #[tokio::test]
    async fn eviction_drops_failed_runs_first_and_runs_in_flight_last() {
        let cache = Arc::new(TestCache::new(2, None));
        let release = Arc::new(tokio::sync::Notify::new());
        let slow = {
            let (cache, release) = (cache.clone(), release.clone());
            tokio::spawn(async move {
                cache
                    .get_or_compute(1, "slow", no_wait, move |()| async move {
                        release.notified().await;
                        Ok::<_, TestError>(7u32)
                    })
                    .await
            })
        };
        let contains = |key: &str| cache.state.lock().unwrap().entries.contains_key(key);
        while !contains("slow") {
            tokio::task::yield_now().await;
        }

        // The oldest entry is in flight, so the finished one makes room.
        assert_eq!(ready(&cache, 1, 1).await, 1);
        let failed = cache
            .get_or_compute(1, "failed", no_wait, |()| async { Err(TestError::Failed) })
            .await;
        assert_eq!(failed, Err(TestError::Failed));
        assert!(!contains("k") && contains("slow"));

        // A failure goes before anything else, however recent.
        cache
            .get_or_compute(1, "new", no_wait, |()| async { Ok::<_, TestError>(2u32) })
            .await
            .unwrap();
        assert!(!contains("failed") && contains("slow"));

        release.notify_one();
        assert_eq!(slow.await.unwrap(), Ok(7));
        assert_eq!(cache.get(1, &"slow"), Some(7), "single flight survived");
        assert_eq!(cache.get(1, &"new"), Some(2));
    }

    #[tokio::test]
    async fn eviction_drops_an_abandoned_run_before_a_finished_answer() {
        let cache = Arc::new(TestCache::new(2, None));
        let queue = Queue::busy();
        let runs = Arc::new(AtomicUsize::new(0));
        assert_eq!(ready(&cache, 1, 1).await, 1);
        let gone = {
            let (cache, admit, compute) = (cache.clone(), queue.admit(), counted(&runs, 2));
            tokio::spawn(async move { cache.get_or_compute(1, "gone", admit, compute).await })
        };
        until(|| queue.waiting() == 1).await;
        gone.abort();
        assert!(gone.await.unwrap_err().is_cancelled());

        // Older and finished, "k" still outranks the abandoned run.
        cache
            .get_or_compute(1, "new", no_wait, |()| async { Ok::<_, TestError>(3u32) })
            .await
            .unwrap();
        let contains = |key: &str| cache.state.lock().unwrap().entries.contains_key(key);
        assert!(contains("k") && !contains("gone") && contains("new"));
        assert_eq!(runs.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_queued_run_nobody_waits_for_gives_up_its_place_and_never_computes() {
        let cache = Arc::new(TestCache::new(4, None));
        let queue = Queue::busy();
        let runs = Arc::new(AtomicUsize::new(0));
        // Version 1 is shared; version 0 is then a stale caller's private run.
        for version in [1, 0] {
            let caller = {
                let (cache, admit, compute) = (cache.clone(), queue.admit(), counted(&runs, 7));
                tokio::spawn(
                    async move { cache.get_or_compute(version, "k", admit, compute).await },
                )
            };
            until(|| queue.waiting() == 1).await;
            caller.abort();
            assert!(caller.await.unwrap_err().is_cancelled());
            until(|| queue.waiting() == 0).await;
        }

        queue.open();
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(runs.load(Ordering::SeqCst), 0, "nobody computed");
        assert_eq!(queue.slot.available_permits(), 1, "nobody took the slot");
        assert_eq!(cache.get(1, &"k"), None);
    }

    #[tokio::test]
    async fn a_queued_run_carries_on_while_another_caller_still_waits() {
        let cache = Arc::new(TestCache::new(4, None));
        let queue = Queue::busy();
        let runs = Arc::new(AtomicUsize::new(0));
        let call = || {
            let (cache, admit, compute) = (cache.clone(), queue.admit(), counted(&runs, 7));
            tokio::spawn(async move { cache.get_or_compute(1, "k", admit, compute).await })
        };
        let first = call();
        until(|| queue.waiting() == 1).await;
        let second = call();
        until(|| interest(&cache, "k") == 2).await;

        first.abort();
        assert!(first.await.unwrap_err().is_cancelled());
        assert_eq!(interest(&cache, "k"), 1);
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(queue.waiting(), 1, "the run kept its place");

        queue.open();
        assert_eq!(second.await.unwrap(), Ok(7));
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        assert_eq!(cache.get(1, &"k"), Some(7));
    }

    #[tokio::test]
    async fn a_caller_after_an_abandoned_run_computes_afresh() {
        let cache = Arc::new(TestCache::new(4, None));
        let runs = Arc::new(AtomicUsize::new(0));
        let call = |queue: &Queue, key: &'static str, value: u32| {
            let (cache, admit, compute) = (cache.clone(), queue.admit(), counted(&runs, value));
            tokio::spawn(async move { cache.get_or_compute(1, key, admit, compute).await })
        };

        // The abandoned run has finished giving up.
        let queue = Queue::busy();
        let gone = call(&queue, "k", 7);
        until(|| queue.waiting() == 1).await;
        gone.abort();
        assert!(gone.await.unwrap_err().is_cancelled());
        until(|| queue.waiting() == 0).await;
        queue.open();
        assert_eq!(call(&queue, "k", 7).await.unwrap(), Ok(7));
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        assert_eq!(cache.get(1, &"k"), Some(7));

        // The new caller arrives before the abandoned run has noticed: one of
        // the two runs computes, once, for it.
        let queue = Queue::busy();
        let gone = call(&queue, "j", 8);
        until(|| queue.waiting() == 1).await;
        gone.abort();
        assert!(gone.await.unwrap_err().is_cancelled());
        let next = call(&queue, "j", 8);
        queue.open();
        assert_eq!(next.await.unwrap(), Ok(8));
        assert_eq!(runs.load(Ordering::SeqCst), 2);
        assert_eq!(cache.get(1, &"j"), Some(8));
        assert_eq!(queue.waiting(), 0);
    }

    #[tokio::test]
    async fn an_admitted_run_finishes_and_is_cached_after_every_caller_leaves() {
        let cache = Arc::new(TestCache::new(4, None));
        let queue = Queue::busy();
        queue.open();
        let started = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(tokio::sync::Notify::new());
        let caller = {
            let (cache, admit) = (cache.clone(), queue.admit());
            let (started, release) = (started.clone(), release.clone());
            tokio::spawn(async move {
                cache
                    .get_or_compute(1, "k", admit, move |slot| async move {
                        let _slot = slot;
                        started.fetch_add(1, Ordering::SeqCst);
                        release.notified().await;
                        Ok(7)
                    })
                    .await
            })
        };
        until(|| started.load(Ordering::SeqCst) == 1).await;
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        assert_eq!(interest(&cache, "k"), 0);

        release.notify_one();
        until(|| cache.get(1, &"k") == Some(7)).await;
        assert_eq!(started.load(Ordering::SeqCst), 1);
        assert_eq!(queue.slot.available_permits(), 1, "the slot came back");
    }
}

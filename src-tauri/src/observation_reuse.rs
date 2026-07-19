use std::{
    sync::{Arc, Condvar, Mutex, MutexGuard},
    time::{Duration, Instant},
};

pub(crate) struct ObservationReuse<T> {
    ttl: Duration,
    state: Mutex<ReuseState<T>>,
    ready: Condvar,
}

enum ReuseState<T> {
    Empty,
    Observing,
    Ready {
        observed_at: Instant,
        observation: Result<Arc<T>, String>,
    },
}

impl<T> ObservationReuse<T> {
    pub(crate) fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            state: Mutex::new(ReuseState::Empty),
            ready: Condvar::new(),
        }
    }

    pub(crate) fn get_or_observe(
        &self,
        observe: impl FnOnce() -> Result<T, String>,
    ) -> Result<Arc<T>, String> {
        let mut observe = Some(observe);
        let mut state = self.lock();

        loop {
            match &*state {
                ReuseState::Ready {
                    observed_at,
                    observation,
                } if observed_at.elapsed() < self.ttl => return observation.clone(),
                ReuseState::Observing => {
                    state = self.wait(state);
                }
                ReuseState::Empty | ReuseState::Ready { .. } => {
                    *state = ReuseState::Observing;
                    drop(state);

                    let observation =
                        observe.take().expect("observation loader is consumed once")()
                            .map(Arc::new);
                    let mut state = self.lock();
                    *state = ReuseState::Ready {
                        observed_at: Instant::now(),
                        observation: observation.clone(),
                    };
                    self.ready.notify_all();
                    return observation;
                }
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, ReuseState<T>> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn wait<'a>(&self, state: MutexGuard<'a, ReuseState<T>>) -> MutexGuard<'a, ReuseState<T>> {
        self.ready
            .wait(state)
            .unwrap_or_else(|error| error.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            Barrier,
        },
        thread,
    };

    #[test]
    fn concurrent_misses_share_one_observation() {
        let reuse = Arc::new(ObservationReuse::new(Duration::from_secs(60)));
        let starts = Arc::new(Barrier::new(8));
        let observations = Arc::new(AtomicUsize::new(0));
        let mut threads = Vec::new();

        for _ in 0..8 {
            let reuse = reuse.clone();
            let starts = starts.clone();
            let observations = observations.clone();
            threads.push(thread::spawn(move || {
                starts.wait();
                reuse
                    .get_or_observe(|| {
                        observations.fetch_add(1, Ordering::SeqCst);
                        thread::sleep(Duration::from_millis(25));
                        Ok(42)
                    })
                    .expect("observation succeeds")
            }));
        }

        for thread in threads {
            assert_eq!(*thread.join().expect("thread joins"), 42);
        }
        assert_eq!(observations.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn expired_observations_are_reloaded() {
        let reuse = ObservationReuse::new(Duration::ZERO);
        let observations = AtomicUsize::new(0);

        for expected in [1, 2] {
            let observed = reuse
                .get_or_observe(|| Ok(observations.fetch_add(1, Ordering::SeqCst) + 1))
                .expect("observation succeeds");
            assert_eq!(*observed, expected);
        }
    }
}

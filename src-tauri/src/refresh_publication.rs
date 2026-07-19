use crate::usage::{Service, UsageDisplayState, UsageRefreshStatus, UsageSource};
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RefreshScope {
    pub(crate) service: Option<Service>,
    pub(crate) source: Option<UsageSource>,
}

impl RefreshScope {
    pub(crate) const ALL: Self = Self {
        service: None,
        source: None,
    };

    pub(crate) const fn provider(service: Service, source: UsageSource) -> Self {
        Self {
            service: Some(service),
            source: Some(source),
        }
    }
}

pub(crate) trait PublicationEffects<E> {
    fn emit_lifecycle(&self, scope: RefreshScope, status: UsageRefreshStatus) -> Result<(), E>;
    fn emit_snapshots_updated(&self, display_state: &UsageDisplayState) -> Result<(), E>;
    fn record_history(&self, display_state: &UsageDisplayState) -> Result<(), E>;
    fn persist_raw_snapshots(&self, display_state: &UsageDisplayState) -> Result<(), E>;
    fn play_sound_cues(&self, display_state: &UsageDisplayState) -> Result<(), E>;
    fn surface_provider_errors(&self, display_state: &UsageDisplayState) -> Result<(), E>;
}

pub(crate) struct RefreshPublicationPolicy {
    active: Mutex<bool>,
}

impl RefreshPublicationPolicy {
    pub(crate) fn new() -> Self {
        Self {
            active: Mutex::new(true),
        }
    }

    pub(crate) fn refresh<E>(
        &self,
        effects: &impl PublicationEffects<E>,
        scope: RefreshScope,
        unavailable: impl Fn() -> E,
        refresh: impl FnOnce() -> Result<UsageDisplayState, E>,
    ) -> Result<UsageDisplayState, E> {
        let active = self.active.lock().map_err(|_| unavailable())?;
        if !*active {
            return Err(unavailable());
        }

        effects.emit_lifecycle(scope, UsageRefreshStatus::Started)?;

        let display_state = match refresh() {
            Ok(display_state) => display_state,
            Err(error) => {
                let _ = effects.emit_lifecycle(scope, UsageRefreshStatus::Failed);
                return Err(error);
            }
        };

        if let Err(error) = effects.emit_snapshots_updated(&display_state) {
            let _ = effects.emit_lifecycle(scope, UsageRefreshStatus::Failed);
            return Err(error);
        }

        let _ = effects.record_history(&display_state);
        let _ = effects.persist_raw_snapshots(&display_state);
        let _ = effects.play_sound_cues(&display_state);
        let _ = effects.surface_provider_errors(&display_state);

        effects.emit_lifecycle(scope, UsageRefreshStatus::Finished)?;
        Ok(display_state)
    }

    pub(crate) fn clear_cache<E>(
        &self,
        effects: &impl PublicationEffects<E>,
        unavailable: impl Fn() -> E,
        clear: impl FnOnce() -> Result<UsageDisplayState, E>,
    ) -> Result<UsageDisplayState, E> {
        let active = self.active.lock().map_err(|_| unavailable())?;
        if !*active {
            return Err(unavailable());
        }

        let display_state = clear()?;
        effects.emit_snapshots_updated(&display_state)?;
        Ok(display_state)
    }

    pub(crate) fn shutdown(&self) {
        if let Ok(mut active) = self.active.lock() {
            *active = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{mpsc, Arc, Mutex},
        thread,
        time::Duration,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Effect {
        Lifecycle(UsageRefreshStatus),
        SnapshotsUpdated,
        History,
        RawSnapshots,
        SoundCues,
        ProviderErrors,
    }

    #[derive(Default)]
    struct RecordingEffects {
        calls: Mutex<Vec<Effect>>,
        failing_effect: Mutex<Option<Effect>>,
    }

    impl RecordingEffects {
        fn fail_on(effect: Effect) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                failing_effect: Mutex::new(Some(effect)),
            }
        }

        fn record(&self, effect: Effect) -> Result<(), &'static str> {
            self.calls.lock().expect("calls lock succeeds").push(effect);
            if self
                .failing_effect
                .lock()
                .expect("failure lock succeeds")
                .as_ref()
                == Some(&effect)
            {
                Err("effect failed")
            } else {
                Ok(())
            }
        }

        fn calls(&self) -> Vec<Effect> {
            self.calls.lock().expect("calls lock succeeds").clone()
        }
    }

    impl PublicationEffects<&'static str> for RecordingEffects {
        fn emit_lifecycle(
            &self,
            _scope: RefreshScope,
            status: UsageRefreshStatus,
        ) -> Result<(), &'static str> {
            self.record(Effect::Lifecycle(status))
        }

        fn emit_snapshots_updated(
            &self,
            _display_state: &UsageDisplayState,
        ) -> Result<(), &'static str> {
            self.record(Effect::SnapshotsUpdated)
        }

        fn record_history(&self, _display_state: &UsageDisplayState) -> Result<(), &'static str> {
            self.record(Effect::History)
        }

        fn persist_raw_snapshots(
            &self,
            _display_state: &UsageDisplayState,
        ) -> Result<(), &'static str> {
            self.record(Effect::RawSnapshots)
        }

        fn play_sound_cues(&self, _display_state: &UsageDisplayState) -> Result<(), &'static str> {
            self.record(Effect::SoundCues)
        }

        fn surface_provider_errors(
            &self,
            _display_state: &UsageDisplayState,
        ) -> Result<(), &'static str> {
            self.record(Effect::ProviderErrors)
        }
    }

    fn display_state() -> UsageDisplayState {
        UsageDisplayState {
            snapshots: Vec::new(),
            updated_at: "2026-07-19T12:00:00Z".to_string(),
        }
    }

    fn expected_refresh_order() -> Vec<Effect> {
        vec![
            Effect::Lifecycle(UsageRefreshStatus::Started),
            Effect::SnapshotsUpdated,
            Effect::History,
            Effect::RawSnapshots,
            Effect::SoundCues,
            Effect::ProviderErrors,
            Effect::Lifecycle(UsageRefreshStatus::Finished),
        ]
    }

    #[test]
    fn manual_scheduled_startup_and_targeted_refreshes_share_effect_order() {
        for (path, scope) in [
            ("startup", RefreshScope::ALL),
            ("manual", RefreshScope::ALL),
            ("scheduled", RefreshScope::ALL),
            (
                "targeted",
                RefreshScope::provider(Service::Codex, UsageSource::Web),
            ),
        ] {
            let policy = RefreshPublicationPolicy::new();
            let effects = RecordingEffects::default();

            policy
                .refresh(&effects, scope, || "unavailable", || Ok(display_state()))
                .expect("refresh publishes");

            assert_eq!(effects.calls(), expected_refresh_order(), "{path}");
        }
    }

    #[test]
    fn cache_clear_publication_is_emit_only() {
        let policy = RefreshPublicationPolicy::new();
        let effects = RecordingEffects::default();

        policy
            .clear_cache(&effects, || "unavailable", || Ok(display_state()))
            .expect("cache clear publishes");

        assert_eq!(effects.calls(), vec![Effect::SnapshotsUpdated]);
    }

    #[test]
    fn nonfatal_history_cache_cue_and_provider_error_failures_do_not_fail_refresh() {
        for failure in [
            Effect::History,
            Effect::RawSnapshots,
            Effect::SoundCues,
            Effect::ProviderErrors,
        ] {
            let policy = RefreshPublicationPolicy::new();
            let effects = RecordingEffects::fail_on(failure);

            policy
                .refresh(
                    &effects,
                    RefreshScope::ALL,
                    || "unavailable",
                    || Ok(display_state()),
                )
                .expect("nonfatal effect failure keeps refresh successful");

            assert_eq!(effects.calls(), expected_refresh_order());
        }
    }

    #[test]
    fn accepted_state_emit_failure_still_has_one_failed_terminal_event() {
        let policy = RefreshPublicationPolicy::new();
        let effects = RecordingEffects::fail_on(Effect::SnapshotsUpdated);

        let error = policy
            .refresh(
                &effects,
                RefreshScope::ALL,
                || "unavailable",
                || Ok(display_state()),
            )
            .expect_err("snapshot emit fails publication");

        assert_eq!(error, "effect failed");
        assert_eq!(
            effects.calls(),
            vec![
                Effect::Lifecycle(UsageRefreshStatus::Started),
                Effect::SnapshotsUpdated,
                Effect::Lifecycle(UsageRefreshStatus::Failed),
            ]
        );
    }

    #[test]
    fn refresh_failure_emits_one_failed_terminal_event() {
        let policy = RefreshPublicationPolicy::new();
        let effects = RecordingEffects::default();

        let error = policy
            .refresh(
                &effects,
                RefreshScope::ALL,
                || "unavailable",
                || Err("refresh failed"),
            )
            .expect_err("refresh fails");

        assert_eq!(error, "refresh failed");
        assert_eq!(
            effects.calls(),
            vec![
                Effect::Lifecycle(UsageRefreshStatus::Started),
                Effect::Lifecycle(UsageRefreshStatus::Failed),
            ]
        );
    }

    #[test]
    fn terminal_emit_failure_is_not_retried_as_a_second_terminal_event() {
        let policy = RefreshPublicationPolicy::new();
        let effects = RecordingEffects::fail_on(Effect::Lifecycle(UsageRefreshStatus::Finished));

        let error = policy
            .refresh(
                &effects,
                RefreshScope::ALL,
                || "unavailable",
                || Ok(display_state()),
            )
            .expect_err("terminal emit fails publication");

        assert_eq!(error, "effect failed");
        assert_eq!(effects.calls(), expected_refresh_order());
    }

    #[test]
    fn started_emit_failure_runs_neither_refresh_nor_terminal_effect() {
        let policy = RefreshPublicationPolicy::new();
        let effects = RecordingEffects::fail_on(Effect::Lifecycle(UsageRefreshStatus::Started));

        let error = policy
            .refresh(
                &effects,
                RefreshScope::ALL,
                || "unavailable",
                || panic!("refresh must not run without a started event"),
            )
            .expect_err("started emit fails publication");

        assert_eq!(error, "effect failed");
        assert_eq!(
            effects.calls(),
            vec![Effect::Lifecycle(UsageRefreshStatus::Started)]
        );
    }

    #[test]
    fn concurrent_refreshes_publish_complete_chains_in_acceptance_order() {
        let policy = Arc::new(RefreshPublicationPolicy::new());
        let effects = Arc::new(RecordingEffects::default());
        let (first_entered_tx, first_entered_rx) = mpsc::channel();
        let (release_first_tx, release_first_rx) = mpsc::channel();

        let first_policy = Arc::clone(&policy);
        let first_effects = Arc::clone(&effects);
        let first = thread::spawn(move || {
            first_policy.refresh(
                first_effects.as_ref(),
                RefreshScope::ALL,
                || "unavailable",
                || {
                    first_entered_tx
                        .send(())
                        .expect("first refresh entered sends");
                    release_first_rx
                        .recv()
                        .expect("first refresh release arrives");
                    Ok(display_state())
                },
            )
        });
        first_entered_rx.recv().expect("first refresh enters");

        let (second_attempted_tx, second_attempted_rx) = mpsc::channel();
        let (second_entered_tx, second_entered_rx) = mpsc::channel();
        let second_policy = Arc::clone(&policy);
        let second_effects = Arc::clone(&effects);
        let second = thread::spawn(move || {
            second_attempted_tx
                .send(())
                .expect("second refresh attempt sends");
            second_policy.refresh(
                second_effects.as_ref(),
                RefreshScope::provider(Service::Codex, UsageSource::Web),
                || "unavailable",
                || {
                    second_entered_tx
                        .send(())
                        .expect("second refresh entered sends");
                    Ok(display_state())
                },
            )
        });
        second_attempted_rx.recv().expect("second refresh attempts");
        assert!(second_entered_rx
            .recv_timeout(Duration::from_millis(25))
            .is_err());

        release_first_tx
            .send(())
            .expect("first refresh release sends");
        first
            .join()
            .expect("first refresh thread joins")
            .expect("first refresh publishes");
        second_entered_rx
            .recv()
            .expect("second refresh enters after first");
        second
            .join()
            .expect("second refresh thread joins")
            .expect("second refresh publishes");

        let mut expected = expected_refresh_order();
        expected.extend(expected_refresh_order());
        assert_eq!(effects.calls(), expected);
    }

    #[test]
    fn cache_clear_waits_for_inflight_refresh_before_its_emit_only_publication() {
        let policy = Arc::new(RefreshPublicationPolicy::new());
        let effects = Arc::new(RecordingEffects::default());
        let (refresh_entered_tx, refresh_entered_rx) = mpsc::channel();
        let (release_refresh_tx, release_refresh_rx) = mpsc::channel();

        let refresh_policy = Arc::clone(&policy);
        let refresh_effects = Arc::clone(&effects);
        let refresh = thread::spawn(move || {
            refresh_policy.refresh(
                refresh_effects.as_ref(),
                RefreshScope::ALL,
                || "unavailable",
                || {
                    refresh_entered_tx.send(()).expect("refresh entered sends");
                    release_refresh_rx.recv().expect("refresh release arrives");
                    Ok(display_state())
                },
            )
        });
        refresh_entered_rx.recv().expect("refresh enters");

        let (clear_attempted_tx, clear_attempted_rx) = mpsc::channel();
        let (clear_entered_tx, clear_entered_rx) = mpsc::channel();
        let clear_policy = Arc::clone(&policy);
        let clear_effects = Arc::clone(&effects);
        let clear = thread::spawn(move || {
            clear_attempted_tx.send(()).expect("clear attempt sends");
            clear_policy.clear_cache(
                clear_effects.as_ref(),
                || "unavailable",
                || {
                    clear_entered_tx.send(()).expect("clear entered sends");
                    Ok(display_state())
                },
            )
        });
        clear_attempted_rx.recv().expect("clear attempts");
        assert!(clear_entered_rx
            .recv_timeout(Duration::from_millis(25))
            .is_err());

        release_refresh_tx.send(()).expect("refresh release sends");
        refresh
            .join()
            .expect("refresh thread joins")
            .expect("refresh publishes");
        clear_entered_rx.recv().expect("clear enters after refresh");
        clear
            .join()
            .expect("clear thread joins")
            .expect("clear publishes");

        let mut expected = expected_refresh_order();
        expected.push(Effect::SnapshotsUpdated);
        assert_eq!(effects.calls(), expected);
    }

    #[test]
    fn shutdown_prevents_refresh_and_cache_clear_effects() {
        let policy = RefreshPublicationPolicy::new();
        let effects = RecordingEffects::default();
        policy.shutdown();

        assert_eq!(
            policy.refresh(
                &effects,
                RefreshScope::ALL,
                || "publication stopped",
                || panic!("refresh must not run after shutdown"),
            ),
            Err("publication stopped")
        );
        assert_eq!(
            policy.clear_cache(
                &effects,
                || "publication stopped",
                || panic!("cache clear must not run after shutdown"),
            ),
            Err("publication stopped")
        );
        assert!(effects.calls().is_empty());
    }
}

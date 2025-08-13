//! Deterministic replay engine for WAL events

use anyhow::Result;
use bus::{Message, Publisher};
use common::Ts;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use storage::{Wal, WalEvent};
use tokio::sync::{Mutex, Notify};
use tokio::time::{Duration, Instant, sleep};
use tracing::{debug, info, trace};

/// Wrapper for `WalEvent` to implement Message trait
#[derive(Debug, Clone)]
pub struct ReplayEvent(pub WalEvent);

impl Message for ReplayEvent {}

impl From<WalEvent> for ReplayEvent {
    fn from(event: WalEvent) -> Self {
        Self(event)
    }
}

impl ReplayEvent {
    /// Get the inner `WalEvent`
    #[must_use]
    pub fn into_inner(self) -> WalEvent {
        self.0
    }

    /// Get timestamp from inner event
    #[must_use]
    pub const fn timestamp(&self) -> Ts {
        self.0.timestamp()
    }
}

/// Replay configuration
#[derive(Debug, Clone)]
pub struct ReplayConfig {
    /// Start timestamp (None = from beginning)
    pub from_ts: Option<Ts>,
    /// End timestamp (None = to end)
    pub to_ts: Option<Ts>,
    /// Playback speed multiplier (1.0 = realtime, 0.0 = fast-forward)
    pub speed: f64,
    /// Whether to loop when reaching the end
    pub loop_replay: bool,
}

impl Default for ReplayConfig {
    fn default() -> Self {
        Self {
            from_ts: None,
            to_ts: None,
            speed: 0.0, // Fast-forward by default
            loop_replay: false,
        }
    }
}

/// Replay status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayStatus {
    /// Replay is idle
    Idle,
    /// Replay is running
    Running,
    /// Replay is paused
    Paused,
    /// Replay has finished
    Finished,
}

/// Replayer for WAL events
pub struct Replayer<P: Publisher<ReplayEvent> + Send + Sync> {
    wal_path: PathBuf,
    publisher: P,
    config: Arc<Mutex<ReplayConfig>>,
    status: Arc<Mutex<ReplayStatus>>,
    pause_notify: Arc<Notify>,
    progress: Arc<Mutex<ReplayProgress>>,
}

/// Replay progress tracking
#[derive(Debug, Clone)]
struct ReplayProgress {
    events_processed: u64,
    current_ts: Option<Ts>,
    start_time: Option<Instant>,
}

/// Internal state for replay loop
struct ReplayLoopState<P> {
    wal_path: PathBuf,
    publisher: P,
    config: Arc<Mutex<ReplayConfig>>,
    status: Arc<Mutex<ReplayStatus>>,
    pause_notify: Arc<Notify>,
    progress: Arc<Mutex<ReplayProgress>>,
}

impl<P: Publisher<ReplayEvent> + Clone + Send + Sync + 'static> Replayer<P> {
    /// Create a new replayer
    ///
    /// # Errors
    ///
    /// Returns an error if the WAL path is invalid.
    pub fn new(wal_path: &Path, publisher: P, config: ReplayConfig) -> Result<Self> {
        Ok(Self {
            wal_path: wal_path.to_path_buf(),
            publisher,
            config: Arc::new(Mutex::new(config)),
            status: Arc::new(Mutex::new(ReplayStatus::Idle)),
            pause_notify: Arc::new(Notify::new()),
            progress: Arc::new(Mutex::new(ReplayProgress {
                events_processed: 0,
                current_ts: None,
                start_time: None,
            })),
        })
    }

    /// Start replay
    ///
    /// # Errors
    ///
    /// Returns an error if replay cannot be started or WAL reading fails.
    pub async fn start(&self) -> Result<()> {
        {
            let mut status = self.status.lock().await;
            if *status != ReplayStatus::Idle && *status != ReplayStatus::Finished {
                return Ok(());
            }
            *status = ReplayStatus::Running;
        }

        info!("Starting replay");

        let wal_path = self.wal_path.clone();
        let publisher = self.publisher.clone();
        let config = self.config.clone();
        let status = self.status.clone();
        let pause_notify = self.pause_notify.clone();
        let progress = self.progress.clone();

        // Reset progress
        {
            let mut prog = progress.lock().await;
            prog.events_processed = 0;
            prog.current_ts = None;
            prog.start_time = Some(Instant::now());
        }

        // Spawn replay task
        tokio::spawn(async move {
            let state = ReplayLoopState {
                wal_path,
                publisher,
                config,
                status: status.clone(),
                pause_notify,
                progress,
            };

            if let Err(e) = Self::replay_loop(state).await {
                tracing::error!("Replay error: {}", e);
            }

            let mut status = status.lock().await;
            if *status == ReplayStatus::Running {
                *status = ReplayStatus::Finished;
            }
        });

        Ok(())
    }

    /// Pause replay
    ///
    /// # Errors
    ///
    /// This function currently never returns an error.
    pub async fn pause(&self) -> Result<()> {
        let mut status = self.status.lock().await;
        if *status == ReplayStatus::Running {
            *status = ReplayStatus::Paused;
            info!("Replay paused");
        }
        drop(status);
        Ok(())
    }

    /// Resume replay
    ///
    /// # Errors
    ///
    /// This function currently never returns an error.
    pub async fn resume(&self) -> Result<()> {
        let mut status = self.status.lock().await;
        if *status == ReplayStatus::Paused {
            *status = ReplayStatus::Running;
            drop(status);
            self.pause_notify.notify_one();
            info!("Replay resumed");
        }
        Ok(())
    }

    /// Stop replay
    ///
    /// # Errors
    ///
    /// This function currently never returns an error.
    pub async fn stop(&self) -> Result<()> {
        {
            let mut status = self.status.lock().await;
            *status = ReplayStatus::Idle;
        }
        self.pause_notify.notify_one();
        info!("Replay stopped");
        Ok(())
    }

    /// Set playback speed
    ///
    /// # Errors
    ///
    /// This function currently never returns an error.
    pub async fn set_speed(&self, speed: f64) -> Result<()> {
        {
            let mut config = self.config.lock().await;
            config.speed = speed;
        }
        debug!("Playback speed set to {}x", speed);
        Ok(())
    }

    /// Get current status
    #[must_use]
    pub async fn status(&self) -> ReplayStatus {
        *self.status.lock().await
    }

    /// Get progress
    #[must_use]
    pub async fn progress(&self) -> (u64, Option<Ts>) {
        let progress = self.progress.lock().await;
        (progress.events_processed, progress.current_ts)
    }

    /// Internal replay loop
    async fn replay_loop(state: ReplayLoopState<P>) -> Result<()> {
        let ReplayLoopState {
            wal_path,
            publisher,
            config,
            status,
            pause_notify,
            progress,
        } = state;
        loop {
            let cfg = config.lock().await.clone();
            let wal = Wal::new(&wal_path, None)?;
            let mut iter = wal.stream::<WalEvent>(cfg.from_ts)?;
            let mut last_ts: Option<Ts> = None;

            while let Some(event) = iter.read_next_entry()? {
                // Check if we should stop
                {
                    let st = *status.lock().await;
                    if st == ReplayStatus::Idle {
                        return Ok(());
                    }

                    // Handle pause
                    if st == ReplayStatus::Paused {
                        pause_notify.notified().await;
                        continue;
                    }
                }

                // Check end timestamp
                if let Some(to_ts) = cfg.to_ts {
                    if event.timestamp() > to_ts {
                        break;
                    }
                }

                // Calculate delay for realistic playback
                if cfg.speed > 0.0 {
                    if let Some(last) = last_ts {
                        #[allow(clippy::cast_precision_loss)] // Acceptable for timing calculations
                        let delay_diff =
                            event.timestamp().as_nanos().saturating_sub(last.as_nanos());
                        // SAFETY: Cast is safe within expected range
                        let real_delay_ns = (delay_diff / 1_000_000_000) as f64 * 1e9
                            // SAFETY: Cast is safe within expected range
                            + (delay_diff % 1_000_000_000) as f64;
                        let replay_delay_ns = real_delay_ns / cfg.speed;

                        if replay_delay_ns > 0.0 && replay_delay_ns.is_finite() {
                            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                            // SAFETY: Cast is safe within expected range
                            let delay_nanos =
                                u64::try_from(replay_delay_ns.round() as i64).unwrap_or(0);
                            let delay = Duration::from_nanos(delay_nanos);
                            sleep(delay).await;
                        }
                    }
                }

                // Publish event
                publisher.publish(ReplayEvent::from(event.clone()))?;

                // Update progress
                {
                    let mut prog = progress.lock().await;
                    prog.events_processed += 1;
                    prog.current_ts = Some(event.timestamp());
                    drop(prog);
                    last_ts = Some(event.timestamp());
                }

                trace!("Replayed event at {}", event.timestamp());
            }

            // Check if we should loop
            if !cfg.loop_replay {
                break;
            }

            info!("Looping replay");
        }

        let prog = progress.lock().await;
        let events_processed = prog.events_processed;
        let elapsed = prog.start_time.map(|s| s.elapsed()).unwrap_or_default();
        drop(prog);
        info!(
            "Replay finished: {} events in {:?}",
            events_processed, elapsed
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bus::{Bus, Subscriber};
    use storage::{SystemEvent, SystemEventType};
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_replay_fast_forward() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let wal_path = temp_dir.path();

        // Create WAL with events
        {
            let mut wal = Wal::new(wal_path, Some(1024 * 1024))?;

            for i in 0..100 {
                let event = WalEvent::System(SystemEvent {
                    ts: Ts::from_nanos(i * 1000),
                    event_type: SystemEventType::Info,
                    message: format!("Event {}", i),
                });
                wal.append(&event)?;
            }
        }

        // Create bus and replayer
        let bus = Bus::<ReplayEvent>::new(1000);
        let publisher = bus.publisher();
        let subscriber = bus.subscriber();
        let rx = subscriber.subscribe()?;

        let config = ReplayConfig {
            speed: 0.0, // Fast-forward
            ..Default::default()
        };

        let replayer = Replayer::new(wal_path, publisher, config)?;

        // Start replay
        replayer.start().await?;

        // Wait a bit for replay to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Collect events
        let mut events = Vec::with_capacity(1000);
        while let Ok(Some(event)) = rx.try_recv() {
            events.push(event.into_inner());
        }

        assert_eq!(events.len(), 100);
        assert_eq!(events[0].timestamp(), Ts::from_nanos(0));
        assert_eq!(events[99].timestamp(), Ts::from_nanos(99000));

        Ok(())
    }

    #[tokio::test]
    async fn test_replay_with_range() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let wal_path = temp_dir.path();

        // Create WAL with events
        {
            let mut wal = Wal::new(wal_path, Some(1024 * 1024))?;

            for i in 0..50 {
                let event = WalEvent::System(SystemEvent {
                    ts: Ts::from_nanos(i * 100),
                    event_type: SystemEventType::Info,
                    message: format!("Event {}", i),
                });
                wal.append(&event)?;
            }
        }

        // Create bus and replayer
        let bus = Bus::<ReplayEvent>::new(1000);
        let publisher = bus.publisher();
        let subscriber = bus.subscriber();
        let rx = subscriber.subscribe()?;

        let config = ReplayConfig {
            from_ts: Some(Ts::from_nanos(1000)), // Start from event 10
            to_ts: Some(Ts::from_nanos(2000)),   // End at event 20
            speed: 0.0,
            loop_replay: false,
        };

        let replayer = Replayer::new(wal_path, publisher, config)?;
        replayer.start().await?;

        // Wait a bit for replay to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Collect events
        let mut events = Vec::with_capacity(1000);
        while let Ok(Some(event)) = rx.try_recv() {
            events.push(event.into_inner());
        }

        assert_eq!(events.len(), 11); // Events 10-20 inclusive
        assert_eq!(events[0].timestamp(), Ts::from_nanos(1000));
        assert_eq!(events[10].timestamp(), Ts::from_nanos(2000));

        Ok(())
    }
}

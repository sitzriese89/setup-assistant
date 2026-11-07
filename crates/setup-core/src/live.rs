//! Runs a telemetry source on a background thread and keeps an analyzer up to date.
//!
//! A session is split into runs: the driver drives, changes the setup, then
//! starts the next run. Each run gets a fresh analyzer (the car changed) and its
//! own recording file; the previous run's balance is kept for comparison.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::demo::{DemoSource, Variant};
use crate::recording::Recorder;
use crate::rules::RuleBook;
use crate::telemetry::{Analyzer, LiveReport, PhaseBalance, Sample};
use crate::track::{TrackInfo, TrackMap, TrackView};

/// What a game adapter returns on each poll.
pub enum Poll {
    /// The game isn't running (or its data isn't available).
    NotRunning,
    /// Game is running but the player isn't driving (menus, pause, replay).
    Idle,
    /// New samples since the last poll (may be empty).
    Samples(Vec<Sample>),
}

pub trait Source: Send {
    fn poll(&mut self) -> Poll;
    /// The track being driven, once known.
    fn track(&self) -> Option<TrackInfo> {
        None
    }
}

/// A source for platforms or games without telemetry yet.
pub struct NoSource;
impl Source for NoSource {
    fn poll(&mut self) -> Poll {
        Poll::NotRunning
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Connection {
    /// Game not running.
    Waiting,
    /// Game running, player not on track.
    Connected,
    /// Receiving driving data.
    Driving,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveStatus {
    pub demo: bool,
    pub connection: Connection,
    pub run: u32,
    pub report: LiveReport,
    /// Balance of the previous run, to compare against.
    pub previous_balance: Vec<PhaseBalance>,
    /// File the current run is recorded to (real driving only).
    pub recording: Option<String>,
    pub track: Option<TrackView>,
    /// Where the car is on the lap right now (m).
    pub car_at: Option<f32>,
}

struct Inner {
    analyzer: Analyzer,
    demo: Option<DemoSource>,
    connection: Connection,
    last_sample: Option<Instant>,
    run: u32,
    previous_balance: Vec<PhaseBalance>,
    record_dir: Option<PathBuf>,
    session_id: u64,
    recorder: Option<(Recorder, PathBuf)>,
    last_flush: Instant,
    track_dir: Option<PathBuf>,
    track: Option<TrackMap>,
    last_track_save: Instant,
}

impl Inner {
    fn recorder(&mut self) -> Option<&mut Recorder> {
        if self.demo.is_some() {
            return None;
        }
        if self.recorder.is_none() {
            let dir = self.record_dir.as_ref()?;
            let path = dir.join(format!("session-{}-run{}.csv", self.session_id, self.run));
            match Recorder::create(&path) {
                Ok(r) => self.recorder = Some((r, path)),
                Err(e) => {
                    eprintln!("recording disabled: {e}");
                    self.record_dir = None;
                    return None;
                }
            }
        }
        self.recorder.as_mut().map(|(r, _)| r)
    }

    /// Switches to the map of `info` (loading it from disk), saving the previous one.
    fn set_track(&mut self, info: Option<TrackInfo>) {
        let Some(info) = info else { return };
        if self.track.as_ref().is_some_and(|m| m.info.id == info.id) {
            return;
        }
        self.save_track();
        self.track = Some(TrackMap::load_or_new(self.track_dir.as_deref(), info));
    }

    fn save_track(&mut self) {
        if let (Some(dir), Some(map)) = (self.track_dir.as_ref(), self.track.as_ref()) {
            if let Err(e) = map.save(dir) {
                eprintln!("couldn't save track map: {e}");
            }
        }
        self.last_track_save = Instant::now();
    }

    fn close_recording(&mut self) {
        if let Some((mut r, _)) = self.recorder.take() {
            let _ = r.flush();
        }
    }
}

fn demo_variant(run: u32) -> Variant {
    // Run 1 starts loose; after the "fixes" the car pushes.
    if run % 2 == 1 {
        Variant::Loose
    } else {
        Variant::Pushing
    }
}

#[derive(Clone)]
pub struct LiveSession {
    inner: Arc<Mutex<Inner>>,
}

impl LiveSession {
    /// Starts polling `source` about 100 times per second. Real driving is
    /// recorded to `record_dir` when given.
    pub fn start(source: Box<dyn Source>, record_dir: Option<PathBuf>) -> Self {
        Self::start_with_tracks(source, record_dir, None)
    }

    /// Like [`start`](Self::start); track maps are saved in `track_dir`.
    pub fn start_with_tracks(
        mut source: Box<dyn Source>,
        record_dir: Option<PathBuf>,
        track_dir: Option<PathBuf>,
    ) -> Self {
        let inner = Arc::new(Mutex::new(Inner {
            analyzer: Analyzer::default(),
            demo: None,
            connection: Connection::Waiting,
            last_sample: None,
            run: 1,
            previous_balance: Vec::new(),
            record_dir,
            session_id: SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()),
            recorder: None,
            last_flush: Instant::now(),
            track_dir,
            track: None,
            last_track_save: Instant::now(),
        }));
        let shared = inner.clone();
        std::thread::Builder::new()
            .name("telemetry".into())
            .spawn(move || loop {
                {
                    let mut st = shared.lock().unwrap();
                    let poll = match st.demo.as_mut() {
                        Some(demo) => Poll::Samples(demo.due()),
                        None => source.poll(),
                    };
                    let info = match st.demo.as_ref() {
                        Some(demo) => Some(demo.track()),
                        None => source.track(),
                    };
                    st.set_track(info);
                    match poll {
                        Poll::NotRunning => st.connection = Connection::Waiting,
                        Poll::Idle => st.connection = Connection::Connected,
                        Poll::Samples(samples) => {
                            if !samples.is_empty() {
                                st.last_sample = Some(Instant::now());
                            }
                            for s in &samples {
                                st.analyzer.push(s);
                                if let Some(map) = st.track.as_mut() {
                                    map.add(s);
                                }
                                if let Some(r) = st.recorder() {
                                    let _ = r.write(s);
                                }
                            }
                            let fresh = st.last_sample.is_some_and(|t| t.elapsed() < Duration::from_secs(1));
                            st.connection = if fresh { Connection::Driving } else { Connection::Connected };
                        }
                    }
                    if st.track.is_some() && st.last_track_save.elapsed() > Duration::from_secs(30) {
                        st.save_track();
                    }
                    if st.last_flush.elapsed() > Duration::from_secs(2) {
                        if let Some((r, _)) = st.recorder.as_mut() {
                            let _ = r.flush();
                        }
                        st.last_flush = Instant::now();
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
            })
            .expect("failed to start telemetry thread");
        Self { inner }
    }

    pub fn status(&self, rules: &RuleBook) -> LiveStatus {
        let st = self.inner.lock().unwrap();
        let mut report = st.analyzer.report();
        for d in &mut report.detected {
            d.label = rules.symptom_label(&d.symptom);
        }
        if let Some(map) = st.track.as_ref() {
            map.annotate(&mut report);
        }
        for d in &mut report.detected {
            d.label = rules.symptom_label(&d.symptom);
        }
        if let Some(e) = report.last_event.as_mut() {
            e.label = rules.symptom_label(&e.symptom);
        }
        let car_at = report.live.as_ref().and(st.analyzer.last_lap_distance());
        LiveStatus {
            demo: st.demo.is_some(),
            connection: st.connection,
            run: st.run,
            report,
            previous_balance: st.previous_balance.clone(),
            recording: st.recorder.as_ref().map(|(_, p)| p.display().to_string()),
            track: st.track.as_ref().map(|m| m.view()),
            car_at,
        }
    }

    /// Clears what was detected in this run (keeps the run number).
    pub fn reset(&self) {
        let mut st = self.inner.lock().unwrap();
        st.analyzer.reset();
        st.last_sample = None;
    }

    /// The setup was changed: start a fresh analysis and recording, remember this run's balance.
    pub fn next_run(&self) {
        let run = self.inner.lock().unwrap().run + 1;
        self.start_run(run);
    }

    /// Like [`next_run`](Self::next_run) with an explicit run number (the UI keeps the count).
    pub fn start_run(&self, run: u32) {
        let mut st = self.inner.lock().unwrap();
        let report = st.analyzer.report();
        if !report.balance_by_phase.is_empty() {
            st.previous_balance = report.balance_by_phase.clone();
        }
        if let Some(map) = st.track.as_mut() {
            map.add_run(&report);
        }
        st.save_track();
        st.run = run.max(1);
        st.analyzer.reset();
        st.last_sample = None;
        st.close_recording();
        let variant = demo_variant(st.run);
        if let Some(d) = st.demo.as_mut() {
            d.set_variant(variant);
        }
    }

    /// Sets the run number without resetting anything (e.g. after an app restart).
    pub fn set_run(&self, run: u32) {
        let mut st = self.inner.lock().unwrap();
        if st.run != run.max(1) {
            st.run = run.max(1);
            st.close_recording();
        }
    }

    /// Names a corner on the current track map (empty name removes it).
    pub fn rename_corner(&self, number: u32, name: String) {
        let mut st = self.inner.lock().unwrap();
        if let Some(map) = st.track.as_mut() {
            if name.trim().is_empty() {
                map.names.remove(&number);
            } else {
                map.names.insert(number, name.trim().to_string());
            }
        }
        st.save_track();
    }

    /// Back to run 1 with nothing remembered.
    pub fn new_session(&self) {
        let mut st = self.inner.lock().unwrap();
        st.run = 1;
        st.previous_balance.clear();
        st.analyzer.reset();
        st.last_sample = None;
        st.close_recording();
        st.session_id = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        if let Some(d) = st.demo.as_mut() {
            d.set_variant(demo_variant(1));
        }
    }

    /// Demo mode plays synthetic laps (at 2x speed) instead of the game.
    pub fn set_demo(&self, on: bool) {
        let mut st = self.inner.lock().unwrap();
        let variant = demo_variant(st.run);
        st.close_recording();
        st.demo = on.then(|| DemoSource::new(2.0, variant));
        st.analyzer.reset();
        st.last_sample = None;
    }
}

//! C1: a cancel token stops a DWG read at stage, page and record-batch
//! boundaries, on the calling thread, and never yields a partial document.
mod hardening_common;

use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::ThreadId;

use acadrust::entities::{EntityType, Line};
use acadrust::io::limits::CancelToken;
use acadrust::types::Vector3;
use acadrust::{CadDocument, DwgReadOptions, DwgReader, DwgWriter, DxfError, DxfVersion};
use hardening_common::serial;

const LINES: usize = 200_000;
/// Records per pass-2 batch with two worker threads: 512 × 2 × 4.
const BATCH: usize = 4096;

fn lines_dwg(version: DxfVersion, count: usize) -> Vec<u8> {
    let mut doc = CadDocument::with_version(version);
    for i in 0..count {
        let x = (i % 1000) as f64;
        let y = (i / 1000) as f64;
        doc.add_entity(EntityType::Line(Line::from_points(
            Vector3::new(x, y, 0.0),
            Vector3::new(x + 0.5, y + 0.5, 0.0),
        )))
        .unwrap();
    }
    DwgWriter::write_to_vec(&doc).unwrap()
}

fn big_ac1032() -> Vec<u8> {
    static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    BYTES
        .get_or_init(|| lines_dwg(DxfVersion::AC1032, LINES))
        .clone()
}

fn big_ac1021() -> Vec<u8> {
    static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    BYTES
        .get_or_init(|| lines_dwg(DxfVersion::AC1021, LINES))
        .clone()
}

fn two_threads() -> rayon::ThreadPool {
    rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap()
}

fn options(cancel: Option<CancelToken>) -> DwgReadOptions {
    DwgReadOptions {
        failsafe: true,
        cancel,
        ..DwgReadOptions::default()
    }
}

fn histogram(doc: &CadDocument) -> BTreeMap<String, usize> {
    let mut kinds = BTreeMap::new();
    for entity in doc.entities() {
        *kinds.entry(format!("{:?}", std::mem::discriminant(entity))).or_insert(0) += 1;
    }
    kinds
}

#[test]
fn the_token_follows_its_flag_and_hides_its_closure() {
    let _guard = serial();
    let flag = Arc::new(AtomicBool::new(false));
    let token = CancelToken::from_flag(Arc::clone(&flag));
    assert!(!token.is_cancelled());
    flag.store(true, Ordering::SeqCst);
    assert!(token.clone().is_cancelled());
    assert_eq!(format!("{token:?}"), "CancelToken(..)");
    assert!(DwgReadOptions::default().cancel.is_none());
    assert!(DwgReadOptions::failsafe().cancel.is_none());
}

#[test]
fn a_token_set_before_the_read_stops_it_before_any_visit() {
    let _guard = serial();
    let flag = Arc::new(AtomicBool::new(true));
    let visits = AtomicUsize::new(0);
    let result = DwgReader::from_stream_with_options(
        Cursor::new(big_ac1032()),
        options(Some(CancelToken::from_flag(flag))),
    )
    .read_visiting(|_, entity| {
        visits.fetch_add(1, Ordering::SeqCst);
        Some(entity)
    });
    assert!(matches!(result, Err(DxfError::Cancelled)), "{:?}", result.err());
    assert_eq!(visits.load(Ordering::SeqCst), 0);
}

/// Token that fires from its `n`-th poll on, remembering how many entities
/// the visitor had seen at that moment.
fn firing_token(n: usize, visits: &Arc<AtomicUsize>, seen_at_fire: &Arc<AtomicUsize>) -> CancelToken {
    let polls = AtomicUsize::new(0);
    let visits = Arc::clone(visits);
    let seen_at_fire = Arc::clone(seen_at_fire);
    CancelToken::new(move || {
        let poll = polls.fetch_add(1, Ordering::SeqCst) + 1;
        if poll < n {
            return false;
        }
        let _ = seen_at_fire.compare_exchange(
            usize::MAX,
            visits.load(Ordering::SeqCst),
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
        true
    })
}

fn visits_after_firing(pool: &rayon::ThreadPool, token_for: impl Fn(&Arc<AtomicUsize>, &Arc<AtomicUsize>) -> CancelToken) -> usize {
    let visits = Arc::new(AtomicUsize::new(0));
    let seen_at_fire = Arc::new(AtomicUsize::new(usize::MAX));
    let token = token_for(&visits, &seen_at_fire);
    let result = pool.install(|| {
        DwgReader::from_stream_with_options(Cursor::new(big_ac1032()), options(Some(token)))
            .read_visiting(|_, entity| {
                visits.fetch_add(1, Ordering::SeqCst);
                Some(entity)
            })
    });
    assert!(matches!(result, Err(DxfError::Cancelled)), "{:?}", result.err());
    visits.load(Ordering::SeqCst) - seen_at_fire.load(Ordering::SeqCst)
}

#[test]
fn a_token_firing_at_any_of_the_first_twelve_polls_cancels() {
    let _guard = serial();
    let pool = two_threads();
    for n in 1..=12 {
        let after = visits_after_firing(&pool, |visits, seen| firing_token(n, visits, seen));
        assert!(after <= BATCH, "poll {n}: {after} visits after the token fired");
    }
}

#[test]
fn a_flag_set_mid_batch_allows_at_most_the_rest_of_that_batch() {
    let _guard = serial();
    let pool = two_threads();
    let flag = Arc::new(AtomicBool::new(false));
    let token = CancelToken::from_flag(Arc::clone(&flag));
    let visits = AtomicUsize::new(0);
    let set_at = AtomicUsize::new(0);
    let result = pool.install(|| {
        DwgReader::from_stream_with_options(Cursor::new(big_ac1032()), options(Some(token)))
            .read_visiting(|_, entity| {
                let seen = visits.fetch_add(1, Ordering::SeqCst) + 1;
                if seen == 3 * BATCH + 1 {
                    set_at.store(seen, Ordering::SeqCst);
                    flag.store(true, Ordering::SeqCst);
                }
                Some(entity)
            })
    });
    assert!(matches!(result, Err(DxfError::Cancelled)), "{:?}", result.err());
    let after = visits.load(Ordering::SeqCst) - set_at.load(Ordering::SeqCst);
    assert!(after <= BATCH, "{after} visits after the flag was set");
}

#[test]
fn a_token_that_never_fires_changes_nothing() {
    let _guard = serial();
    let pool = two_threads();
    let plain = pool
        .install(|| DwgReader::from_stream_with_options(Cursor::new(big_ac1032()), options(None)).read())
        .unwrap();
    let token = CancelToken::new(|| false);
    let with_token = pool
        .install(|| DwgReader::from_stream_with_options(Cursor::new(big_ac1032()), options(Some(token))).read())
        .unwrap();
    assert_eq!(plain.entities().count(), with_token.entities().count());
    assert_eq!(histogram(&plain), histogram(&with_token));
}

#[test]
fn progress_and_cancel_checks_run_on_the_calling_thread() {
    let _guard = serial();
    let pool = two_threads();
    let threads: Arc<Mutex<Vec<ThreadId>>> = Arc::new(Mutex::new(Vec::new()));
    let token_threads = Arc::clone(&threads);
    let token = CancelToken::new(move || {
        token_threads.lock().unwrap().push(std::thread::current().id());
        false
    });
    let progress_threads = Arc::clone(&threads);
    let caller = pool.install(|| {
        let caller = std::thread::current().id();
        let mut reader =
            DwgReader::from_stream_with_options(Cursor::new(big_ac1032()), options(Some(token)));
        reader.set_progress_callback(Arc::new(move |_| {
            progress_threads.lock().unwrap().push(std::thread::current().id());
        }));
        reader.read().unwrap();
        caller
    });
    let seen = threads.lock().unwrap();
    assert!(seen.len() > 20, "only {} polls and progress reports", seen.len());
    assert!(seen.iter().all(|id| *id == caller), "a check ran off the calling thread");
}

/// One poll of the token: the last progress value reported before it and the
/// number of entities the visitor had seen.
#[derive(Clone, Copy)]
struct Poll {
    progress: u16,
    visits: usize,
}

/// Reads `bytes` with a token that never fires and returns every poll.
fn poll_log(bytes: Vec<u8>) -> Vec<Poll> {
    let pool = two_threads();
    let progress = Arc::new(AtomicU16::new(0));
    let visits = Arc::new(AtomicUsize::new(0));
    let log = Arc::new(Mutex::new(Vec::new()));
    let token = {
        let (progress, visits, log) = (Arc::clone(&progress), Arc::clone(&visits), Arc::clone(&log));
        CancelToken::new(move || {
            log.lock().unwrap().push(Poll {
                progress: progress.load(Ordering::SeqCst),
                visits: visits.load(Ordering::SeqCst),
            });
            false
        })
    };
    pool.install(|| {
        let mut reader = DwgReader::from_stream_with_options(Cursor::new(bytes), options(Some(token)));
        let stored = Arc::clone(&progress);
        reader.set_progress_callback(Arc::new(move |value| stored.store(value, Ordering::SeqCst)));
        reader
            .read_visiting(|_, entity| {
                visits.fetch_add(1, Ordering::SeqCst);
                Some(entity)
            })
            .unwrap();
    });
    let polls = log.lock().unwrap().clone();
    polls
}

#[test]
fn page_batches_and_every_1024_records_of_pass_1_are_polled() {
    let _guard = serial();
    for (label, bytes) in [("AC1032", big_ac1032()), ("AC1021", big_ac1021())] {
        let polls = poll_log(bytes);
        // While the objects section (progress 50) is decompressed, the token
        // is polled once per page batch, besides the stage poll itself.
        let page_polls = polls.iter().filter(|poll| poll.progress == 50).count();
        assert!(page_polls >= 20, "{label}: {page_polls} polls while the objects section was read");
        // Pass 1 of the builder follows, before any entity is visited.
        let pass_1_polls = polls
            .iter()
            .filter(|poll| poll.progress > 50 && poll.visits == 0)
            .count();
        assert!(
            pass_1_polls >= LINES / 1024,
            "{label}: {pass_1_polls} polls in pass 1 of {LINES} records"
        );
    }
}

/// Progress values after which a read polls the token: the start, the stage
/// reports of `DwgReader` and the builder's 875 before the post-passes. The
/// first page batch of the section read that follows 20, 30, 40, 50 and 970
/// polls at the same progress value, so a missing stage poll there only shows
/// as one poll less; 0, 875 and 930 have no such neighbour.
const STAGES: [u16; 8] = [0, 20, 30, 40, 50, 875, 930, 970];

#[test]
fn a_token_firing_right_after_a_stage_report_stops_the_read_at_that_stage() {
    let _guard = serial();
    let pool = two_threads();
    for version in [DxfVersion::AC1032, DxfVersion::AC1021] {
        let bytes = lines_dwg(version, 3_000);
        for failsafe in [true, false] {
            for stage in STAGES {
                let last = Arc::new(AtomicU16::new(0));
                let seen = Arc::clone(&last);
                let token = CancelToken::new(move || seen.load(Ordering::SeqCst) >= stage);
                let result = pool.install(|| {
                    let mut reader = DwgReader::from_stream_with_options(
                        Cursor::new(bytes.clone()),
                        DwgReadOptions {
                            failsafe,
                            cancel: Some(token),
                            ..DwgReadOptions::default()
                        },
                    );
                    let stored = Arc::clone(&last);
                    reader.set_progress_callback(Arc::new(move |value| stored.store(value, Ordering::SeqCst)));
                    reader.read()
                });
                assert!(
                    matches!(result, Err(DxfError::Cancelled)),
                    "{version:?} failsafe={failsafe} stage {stage}: {:?}",
                    result.err()
                );
                assert_eq!(
                    last.load(Ordering::SeqCst),
                    stage,
                    "{version:?} failsafe={failsafe}: the read went on past stage {stage}"
                );
            }
        }
    }
}

#[test]
fn a_token_firing_at_any_poll_ends_the_read_without_another_poll() {
    let _guard = serial();
    let pool = two_threads();
    for version in [DxfVersion::AC1032, DxfVersion::AC1021] {
        // 10,000 records: several pass-1 polls and three pass-2 batches.
        let bytes = lines_dwg(version, 10_000);
        let log = poll_log(bytes.clone());
        let total = log.len();
        assert!(
            log.iter().any(|poll| poll.progress > 50 && poll.visits == 0),
            "{version:?}: no pass-1 poll in {total}"
        );
        assert!(
            log.iter().any(|poll| poll.visits > 0 && poll.progress < 875),
            "{version:?}: no later pass-2 batch poll in {total}"
        );
        for fire_at in 1..=total + 1 {
            let polls = Arc::new(AtomicUsize::new(0));
            let counted = Arc::clone(&polls);
            let token = CancelToken::new(move || counted.fetch_add(1, Ordering::SeqCst) + 1 >= fire_at);
            let result = pool.install(|| {
                DwgReader::from_stream_with_options(Cursor::new(bytes.clone()), options(Some(token))).read()
            });
            if fire_at <= total {
                assert!(
                    matches!(result, Err(DxfError::Cancelled)),
                    "{version:?} poll {fire_at}: {:?}",
                    result.err()
                );
                assert_eq!(
                    polls.load(Ordering::SeqCst),
                    fire_at,
                    "{version:?}: the read polled again after the token fired at poll {fire_at}"
                );
            } else {
                // One more poll than the read makes: the token never fires.
                assert!(result.is_ok(), "{version:?}: {:?}", result.err());
                assert_eq!(polls.load(Ordering::SeqCst), total);
            }
        }
    }
}

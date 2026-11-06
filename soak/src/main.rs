//! The soak run (PLAN 12.10, T7): 64 threads creating, enriching, grouping,
//! rendering, reporting, and dropping errors for the given number of
//! minutes. The first half runs with no creation hook or reporter; at the
//! half both are installed. Every minute the live heap bytes (counted by
//! this program's allocator) and the resident memory go into a CSV file.
//! The run passes when, in each half, the mean of the last ten samples of
//! each is within its limit of the mean of the first ten after warm-up (1 MiB
//! for the live heap, 1% for resident memory), and when the live heap bytes
//! after every worker has stopped are back to what they were before the
//! first started (D-061).
//!
//! `cargo run --release --manifest-path soak/Cargo.toml -- <minutes> <csv>`

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::fmt::{self, Write as _};
use std::io::{self, Write as _};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use define_err::{Annotate, CreationHook, CreationInfo, Event, Reporter};
use report_err::{err, Context, Error, RenderStyle};

const THREADS: usize = 64;

/// Minutes at the start of each half that are not compared for the live
/// heap bytes.
const WARM_UP: usize = 5;

/// How many bytes the live heap may grow between two means of ten samples.
/// Each sample catches the 64 workers part way through a round, with
/// different errors alive, so it moves by tens of percent of its 200 KiB or
/// so. A leak or a cache in any path a round takes grows without bound
/// instead, since the soak runs it billions of times.
const LIVE_GROWTH: f64 = 1048576.0;

/// How far the live heap bytes after the run may be from before it: the
/// buffers the first report and the first line of output make.
const LIVE_SLACK: i64 = 64 * 1024;

/// Counts live heap bytes in one of `SHARDS` counters per thread, so 64
/// workers do not all write one cache line. A block freed on another thread
/// than made it moves bytes between counters; the sum stays exact.
struct Counting;

const SHARDS: usize = 64;

#[repr(align(128))]
struct Shard(AtomicI64);

#[allow(clippy::declare_interior_mutable_const)]
const SHARD: Shard = Shard(AtomicI64::new(0));
static LIVE: [Shard; SHARDS] = [SHARD; SHARDS];
static NEXT_SHARD: AtomicUsize = AtomicUsize::new(0);

std::thread_local! {
    static MY_SHARD: Cell<usize> = const { Cell::new(usize::MAX) };
}

fn count(bytes: i64) {
    let shard = MY_SHARD
        .try_with(|s| {
            if s.get() == usize::MAX {
                s.set(NEXT_SHARD.fetch_add(1, Ordering::Relaxed) % SHARDS);
            }
            s.get()
        })
        .unwrap_or(0);
    LIVE[shard].0.fetch_add(bytes, Ordering::Relaxed);
}

fn live() -> i64 {
    LIVE.iter().map(|s| s.0.load(Ordering::Relaxed)).sum()
}

// SAFETY: forwards every call to the system allocator unchanged; the counters
// are atomics and a thread-local cell with a constant initialiser, which never
// allocate.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: same contract as the caller's.
        let block = unsafe { System.alloc(layout) };
        if !block.is_null() {
            count(layout.size() as i64);
        }
        block
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: same contract as the caller's.
        let block = unsafe { System.alloc_zeroed(layout) };
        if !block.is_null() {
            count(layout.size() as i64);
        }
        block
    }

    unsafe fn realloc(&self, block: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: same contract as the caller's.
        let moved = unsafe { System.realloc(block, layout, new_size) };
        if !moved.is_null() {
            count(new_size as i64 - layout.size() as i64);
        }
        moved
    }

    unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(block, layout) };
        count(-(layout.size() as i64));
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

static STOP: AtomicBool = AtomicBool::new(false);
static OPS: AtomicU64 = AtomicU64::new(0);
static RECEIVED: AtomicU64 = AtomicU64::new(0);

struct Hook;
impl CreationHook for Hook {
    fn on_create(&self, info: &CreationInfo<'_>, out: &mut dyn Annotate) {
        out.field("request_id", &"r-42");
        out.field("type", &info.type_name());
    }
}

struct Count;
impl Reporter for Count {
    fn receive(&self, event: &Event<'_>) {
        if event.record().is_some() {
            RECEIVED.fetch_add(1, Ordering::Relaxed);
        }
    }
}

static HOOK: Hook = Hook;
static COUNT: Count = Count;

struct Sink(usize);
impl fmt::Write for Sink {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0 += s.len();
        Ok(())
    }
}

fn parse(text: &str) -> Result<u32, std::num::ParseIntError> {
    text.parse()
}

/// One round of every operation.
fn round(thread: usize, i: u64) {
    let io = std::io::Error::from(std::io::ErrorKind::NotFound);
    let mut error = Error::new(io)
        .context("reading the config")
        .context(format!("thread {thread} round {i}"))
        .attach(i)
        .attach_display("tenant 7")
        .note("check the path")
        .trace();
    for n in 0..(i % 12) {
        error = error.context(format!("layer {n}"));
    }
    let children = (0..(i % 5)).map(|n| err!("child {n}").context("in a batch"));
    let mut group = Error::group("batch failed", children);
    group.push_child(error);
    let group = group.suppress(err!("cleanup failed"));
    let mut sink = Sink(0);
    for style in [
        RenderStyle::Message,
        RenderStyle::Chain,
        RenderStyle::Report,
    ] {
        let _ = group.render(&mut sink, style);
    }
    let _ = write!(sink, "{group:?}{group:#}");
    let _ = group.find::<std::io::Error>();
    let _ = group.record();
    let _ = group.report();
    let (_odd, _even) = group.split(|e| e.to_string().len() % 2 == 0);
    let items = ["1", "x", "3"];
    let all: report_err::Result<Vec<u32>> =
        report_err::CollectAll::collect_all(items.iter().map(|s| parse(s)));
    let _ = all.context("parsing");
    let _ = report_err::boundary(|| -> report_err::Result<u32> { Ok(1) });
}

/// Resident memory in bytes.
fn resident() -> u64 {
    #[cfg(target_os = "linux")]
    {
        let statm = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
        let pages: u64 = statm
            .split_whitespace()
            .nth(1)
            .and_then(|p| p.parse().ok())
            .unwrap_or(0);
        pages * 4096
    }
    #[cfg(windows)]
    {
        // The working set, from `tasklist` ("12,345 K").
        let pid = std::process::id().to_string();
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        let field = out.rsplit("\",\"").next().unwrap_or("");
        let kib: u64 = field
            .chars()
            .filter(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .unwrap_or(0);
        kib * 1024
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        0
    }
}

fn mean(xs: &[u64]) -> f64 {
    xs.iter().map(|&x| x as f64).sum::<f64>() / xs.len().max(1) as f64
}

/// Whether one series stayed flat: the mean of its last ten samples is at
/// most `ratio` of the mean of its first ten after `warm_up` samples, plus
/// `bytes`, above that mean.
fn flat(name: &str, series: &[u64], warm_up: usize, ratio: f64, bytes: f64) -> bool {
    if series.len() < warm_up + 20 {
        println!("{name}: {} samples, too few to judge", series.len());
        return true;
    }
    let early = mean(&series[warm_up..warm_up + 10]);
    let late = mean(&series[series.len() - 10..]);
    let limit = early * ratio + bytes;
    println!(
        "{name}: {:.2} MiB after warm-up, {:.2} MiB at the end, {:+.0} KiB (limit {:+.0} KiB)",
        early / 1048576.0,
        late / 1048576.0,
        (late - early) / 1024.0,
        limit / 1024.0
    );
    late - early <= limit
}

/// Live heap bytes and resident memory, sampled once a minute.
#[derive(Default)]
struct Half {
    live: Vec<u64>,
    resident: Vec<u64>,
}

impl Half {
    /// Both series flat. The resident memory is the operating system's count
    /// of this process's pages, which also moves as the system allocator's
    /// heap settles; on Windows that took more than an hour (D-061), so a
    /// quarter of the half is warm-up.
    fn flat(&self, name: &str) -> bool {
        let live = flat(
            &format!("{name} live heap"),
            &self.live,
            WARM_UP,
            0.0,
            LIVE_GROWTH,
        );
        let resident = flat(
            &format!("{name} resident"),
            &self.resident,
            (self.resident.len() / 4).max(WARM_UP),
            0.01,
            0.0,
        );
        live & resident
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let minutes: u64 = args.get(1).and_then(|m| m.parse().ok()).unwrap_or(480);
    let csv_path = args.get(2).cloned().unwrap_or_else(|| "soak.csv".into());
    let mut csv = std::fs::File::create(&csv_path).expect("csv file");
    writeln!(
        csv,
        "minute,phase,live_bytes,resident_bytes,operations,records"
    )
    .ok();
    // Whatever the first sample and the first line of output allocate for
    // good is made before the baseline is taken.
    let _ = resident();
    println!("{} live heap bytes before the workers start", live());
    io::stdout().flush().ok();
    let before = live();

    let workers: Vec<_> = (0..THREADS)
        .map(|t| {
            std::thread::spawn(move || {
                let mut i = 0u64;
                while !STOP.load(Ordering::Relaxed) {
                    round(t, i);
                    i += 1;
                    OPS.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();

    let start = Instant::now();
    let half_len = usize::try_from(minutes - minutes / 2).unwrap_or(0) + 1;
    let mut halves: [Half; 2] = Default::default();
    for half in &mut halves {
        half.live.reserve(half_len);
        half.resident.reserve(half_len);
    }
    for minute in 1..=minutes {
        let next = start + Duration::from_secs(minute * 60);
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
        let phase = usize::from(minute > minutes / 2);
        if phase == 1 && define_err::creation_hook().is_none() {
            define_err::set_creation_hook(&HOOK).expect("hook installed once");
            define_err::set_reporter(&COUNT).expect("reporter installed once");
        }
        let rss = resident();
        let heap = u64::try_from(live()).unwrap_or(0);
        halves[phase].live.push(heap);
        halves[phase].resident.push(rss);
        let line = format!(
            "{minute},{},{heap},{rss},{},{}",
            if phase == 0 { "plain" } else { "hook+reporter" },
            OPS.load(Ordering::Relaxed),
            RECEIVED.load(Ordering::Relaxed)
        );
        writeln!(csv, "{line}").ok();
        println!("{line}");
        io::stdout().flush().ok();
    }
    STOP.store(true, Ordering::Relaxed);
    for w in workers {
        w.join().expect("worker panicked");
    }
    let after = live();
    let returned = (after - before).abs() <= LIVE_SLACK;
    println!(
        "live heap: {before} bytes before the workers, {after} after they stopped, {:+} (limit {LIVE_SLACK})",
        after - before
    );
    let ok = halves[0].flat("plain") & halves[1].flat("hook+reporter") & returned;
    println!(
        "{} rounds, {} records received",
        OPS.load(Ordering::Relaxed),
        RECEIVED.load(Ordering::Relaxed)
    );
    if !ok {
        println!("FAILED: memory grew by more than 1%");
        std::process::exit(1);
    }
    println!("PASSED");
}

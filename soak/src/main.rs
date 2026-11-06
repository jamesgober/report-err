//! The soak run (PLAN 12.10, T7): 64 threads creating, enriching, grouping,
//! rendering, reporting, and dropping errors for the given number of
//! minutes. The first half runs with no creation hook or reporter; at the
//! half both are installed. Resident memory is sampled every minute into a
//! CSV file. At the end the run passes when memory after warm-up stayed
//! flat: the mean of the last ten samples of each half is within 1% of the
//! mean of that half's first ten after warm-up.
//!
//! `cargo run --release --manifest-path soak/Cargo.toml -- <minutes> <csv>`

use std::fmt::{self, Write as _};
use std::io::{self, Write as _};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use define_err::{Annotate, CreationHook, CreationInfo, Event, Reporter};
use report_err::{err, Context, Error, RenderStyle};

const THREADS: usize = 64;

/// Minutes at the start of each half that are not compared (allocator
/// warm-up).
const WARM_UP: usize = 5;

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

/// Whether one half stayed flat: its last ten samples against its first ten
/// after warm-up.
fn flat(name: &str, half: &[u64]) -> bool {
    if half.len() < WARM_UP + 20 {
        println!("{name}: {} samples, too few to judge", half.len());
        return true;
    }
    let early = mean(&half[WARM_UP..WARM_UP + 10]);
    let late = mean(&half[half.len() - 10..]);
    let growth = (late - early) / early;
    println!(
        "{name}: {:.1} MiB after warm-up, {:.1} MiB at the end, {:+.2}%",
        early / 1048576.0,
        late / 1048576.0,
        growth * 100.0
    );
    growth <= 0.01
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let minutes: u64 = args.get(1).and_then(|m| m.parse().ok()).unwrap_or(480);
    let csv_path = args.get(2).cloned().unwrap_or_else(|| "soak.csv".into());
    let mut csv = std::fs::File::create(&csv_path).expect("csv file");
    writeln!(csv, "minute,phase,resident_bytes,operations,records").ok();

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
    let mut halves: [Vec<u64>; 2] = [Vec::new(), Vec::new()];
    for minute in 1..=minutes {
        let next = start + Duration::from_secs(minute * 60);
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
        let phase = usize::from(minute > minutes / 2);
        if phase == 1 && define_err::creation_hook().is_none() {
            define_err::set_creation_hook(&HOOK).expect("hook installed once");
            define_err::set_reporter(&COUNT).expect("reporter installed once");
        }
        let rss = resident();
        halves[phase].push(rss);
        let line = format!(
            "{minute},{},{rss},{},{}",
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
    let ok = flat("plain", &halves[0]) & flat("hook+reporter", &halves[1]);
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

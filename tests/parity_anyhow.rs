//! The anyhow parity suite (PLAN 12.7): the same programs, written once,
//! compiled against anyhow and against report-err, give the same `{}`,
//! `{:#}`, chain, root cause, `source()` walk, and downcast results. The
//! known differences (PLAN 8.14.2) are asserted as differences.

use std::error::Error as StdError;
use std::fmt;
use std::io;

#[derive(Debug)]
pub struct Leaf;
impl fmt::Display for Leaf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("leaf failure")
    }
}
impl StdError for Leaf {}

#[derive(Debug)]
pub struct Wrap(pub Leaf);
impl fmt::Display for Wrap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("wrapped")
    }
}
impl StdError for Wrap {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.0)
    }
}

#[derive(Debug, PartialEq)]
pub struct Shard(pub u32);
impl fmt::Display for Shard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "shard {}", self.0)
    }
}

/// A value that is `Display + Debug` but not an error.
#[derive(Debug)]
pub struct NotAnError(pub &'static str);
impl fmt::Display for NotAnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

pub fn missing() -> io::Error {
    io::Error::from(io::ErrorKind::NotFound)
}

/// Everything a program's error is compared on.
#[derive(Debug, PartialEq)]
pub struct Observed {
    pub display: String,
    pub alternate: String,
    pub chain: Vec<String>,
    pub sources: Vec<String>,
    pub root_cause: String,
    pub is_io: bool,
    pub is_static_str: bool,
    pub is_string: bool,
    pub shard: Option<u32>,
    pub is_leaf: bool,
}

macro_rules! programs {
    ($module:ident, $krate:ident, $adhoc:ident) => {
        pub mod $module {
            use ::$krate::$adhoc as adhoc;
            use ::$krate::{bail, ensure, Context, Result};

            use super::*;

            pub fn observe(error: &::$krate::Error) -> Observed {
                let mut sources = vec![];
                let mut cur: Option<&(dyn StdError + 'static)> = Some(error.as_ref());
                while let Some(link) = cur {
                    sources.push(link.to_string());
                    cur = link.source();
                }
                Observed {
                    display: format!("{error}"),
                    alternate: format!("{error:#}"),
                    chain: error.chain().map(|c| c.to_string()).collect(),
                    sources,
                    root_cause: error.root_cause().to_string(),
                    is_io: error.downcast_ref::<io::Error>().is_some(),
                    is_static_str: error.is::<&str>(),
                    is_string: error.is::<String>(),
                    shard: error.downcast_ref::<Shard>().map(|s| s.0),
                    is_leaf: error.is::<Leaf>(),
                }
            }

            pub fn question_mark() -> Result<()> {
                Err(missing())?;
                Ok(())
            }
            pub fn literal_context() -> Result<()> {
                Err(missing()).context("reading the config")?;
                Ok(())
            }
            pub fn lazy_context() -> Result<()> {
                let path = "app.conf";
                Err(missing()).with_context(|| format!("reading {path}"))?;
                Ok(())
            }
            pub fn three_layers() -> Result<()> {
                literal_context()
                    .context("loading settings")
                    .context("starting")?;
                Ok(())
            }
            pub fn nine_layers() -> Result<()> {
                let mut r: Result<()> = Err(Leaf.into());
                for i in 0..9 {
                    r = r.with_context(|| format!("layer {i}"));
                }
                r
            }
            pub fn bail_literal() -> Result<()> {
                bail!("no input files");
            }
            pub fn bail_format() -> Result<()> {
                bail!("port {} is in use", 8080);
            }
            pub fn bail_capture() -> Result<()> {
                let port = 8081;
                bail!("port {port} is in use");
            }
            pub fn ensure_message() -> Result<()> {
                ensure!(1 + 1 == 3, "arithmetic is broken");
                Ok(())
            }
            pub fn ensure_format() -> Result<()> {
                let n = 12;
                ensure!(n <= 10, "batch of {} is larger than {}", n, 10);
                Ok(())
            }
            pub fn option_context() -> Result<()> {
                let found: Option<u8> = None;
                found.context("nothing found")?;
                Ok(())
            }
            pub fn option_lazy_context() -> Result<()> {
                let id = 9;
                let found: Option<u8> = None;
                found.with_context(|| format!("no user {id}"))?;
                Ok(())
            }
            pub fn adhoc_error_value() -> Result<()> {
                Err(adhoc!(missing()))
            }
            pub fn adhoc_string() -> Result<()> {
                Err(adhoc!(String::from("owned message")))
            }
            pub fn adhoc_literal() -> Result<()> {
                Err(adhoc!("literal message"))
            }
            pub fn adhoc_not_an_error() -> Result<()> {
                let r: std::result::Result<(), NotAnError> = Err(NotAnError("table invalid"));
                r.map_err(|e| adhoc!(e)).context("loading signatures")?;
                Ok(())
            }
            pub fn source_chain() -> Result<()> {
                Err(Wrap(Leaf))?;
                Ok(())
            }
            pub fn source_chain_with_context() -> Result<()> {
                Err(Wrap(Leaf)).context("outer")?;
                Ok(())
            }
            pub fn value_context() -> Result<()> {
                Err(missing()).context(Shard(4))?;
                Ok(())
            }
            pub fn value_context_under_more() -> Result<()> {
                value_context().context("rebalancing")?;
                Ok(())
            }
            pub fn parse_error() -> Result<u32> {
                let n: u32 = "x9".parse().context("parsing the count")?;
                Ok(n)
            }
            pub fn error_new() -> Result<()> {
                Err(::$krate::Error::new(Leaf))
            }
            pub fn error_msg() -> Result<()> {
                Err(::$krate::Error::msg("plain message"))
            }
            pub fn error_context_method() -> Result<()> {
                Err(::$krate::Error::new(Leaf)
                    .context("first")
                    .context(String::from("second")))
            }
            pub fn context_on_error_result() -> Result<()> {
                let inner: Result<()> = Err(::$krate::Error::msg("inner"));
                inner.context("outer")?;
                Ok(())
            }
            pub fn ok_value() -> Result<u8> {
                let v = Some(3u8).context("unused")?;
                let w: u8 = Ok::<u8, io::Error>(4).context("unused")?;
                Ok(v + w)
            }
            pub fn format_err_macro() -> Result<()> {
                Err(::$krate::format_err!("code {}", 7))
            }
            pub fn multi_line() -> Result<()> {
                Err(::$krate::Error::msg("line one\nline two")).context("parent")
            }
            pub fn empty_message() -> Result<()> {
                Err(::$krate::Error::msg("")).context("")
            }

            pub fn failing() -> Vec<(&'static str, ::$krate::Error)> {
                vec![
                    ("question_mark", question_mark().unwrap_err()),
                    ("literal_context", literal_context().unwrap_err()),
                    ("lazy_context", lazy_context().unwrap_err()),
                    ("three_layers", three_layers().unwrap_err()),
                    ("nine_layers", nine_layers().unwrap_err()),
                    ("bail_literal", bail_literal().unwrap_err()),
                    ("bail_format", bail_format().unwrap_err()),
                    ("bail_capture", bail_capture().unwrap_err()),
                    ("ensure_message", ensure_message().unwrap_err()),
                    ("ensure_format", ensure_format().unwrap_err()),
                    ("option_context", option_context().unwrap_err()),
                    ("option_lazy_context", option_lazy_context().unwrap_err()),
                    ("adhoc_error_value", adhoc_error_value().unwrap_err()),
                    ("adhoc_string", adhoc_string().unwrap_err()),
                    ("adhoc_literal", adhoc_literal().unwrap_err()),
                    ("adhoc_not_an_error", adhoc_not_an_error().unwrap_err()),
                    ("source_chain", source_chain().unwrap_err()),
                    (
                        "source_chain_with_context",
                        source_chain_with_context().unwrap_err(),
                    ),
                    ("value_context", value_context().unwrap_err()),
                    (
                        "value_context_under_more",
                        value_context_under_more().unwrap_err(),
                    ),
                    ("parse_error", parse_error().unwrap_err()),
                    ("error_new", error_new().unwrap_err()),
                    ("error_msg", error_msg().unwrap_err()),
                    ("error_context_method", error_context_method().unwrap_err()),
                    (
                        "context_on_error_result",
                        context_on_error_result().unwrap_err(),
                    ),
                    ("format_err_macro", format_err_macro().unwrap_err()),
                    ("multi_line", multi_line().unwrap_err()),
                ]
            }
        }
    };
}

programs!(via_anyhow, anyhow, anyhow);
programs!(via_report, report_err, err);

#[test]
fn test_parity_every_program_observes_the_same() {
    let theirs = via_anyhow::failing();
    let ours = via_report::failing();
    assert_eq!(theirs.len(), ours.len());
    for ((name, a), (_, r)) in theirs.iter().zip(ours.iter()) {
        assert_eq!(
            via_anyhow::observe(a),
            via_report::observe(r),
            "program `{name}`"
        );
    }
}

#[test]
fn test_parity_success_paths_are_equal() {
    assert_eq!(via_anyhow::ok_value().ok(), via_report::ok_value().ok());
}

#[test]
fn test_parity_downcast_by_value_to_root_and_context() {
    let a: io::Error = via_anyhow::value_context().unwrap_err().downcast().unwrap();
    let r: io::Error = via_report::value_context().unwrap_err().downcast().unwrap();
    assert_eq!(a.kind(), r.kind());
    let a: Shard = via_anyhow::value_context_under_more()
        .unwrap_err()
        .downcast()
        .unwrap();
    let r: Shard = via_report::value_context_under_more()
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(a, r);
    assert!(via_anyhow::error_new()
        .unwrap_err()
        .downcast::<io::Error>()
        .is_err());
    assert!(via_report::error_new()
        .unwrap_err()
        .downcast::<io::Error>()
        .is_err());
}

/// Differences that are part of the design (PLAN 8.14.2), asserted so a
/// change on either side shows up.
#[test]
fn test_parity_known_differences() {
    fn bare_anyhow() -> anyhow::Result<()> {
        anyhow::ensure!(1 > 2);
        Ok(())
    }
    fn bare_ours() -> report_err::Result<()> {
        report_err::ensure!(1 > 2);
        Ok(())
    }
    assert_eq!(
        bare_ours().unwrap_err().to_string(),
        "condition failed: 1 > 2"
    );
    assert_ne!(
        bare_anyhow().unwrap_err().to_string(),
        bare_ours().unwrap_err().to_string()
    );

    // `{:?}` adds the location block after anyhow's layout.
    let a = format!("{:?}", via_anyhow::literal_context().unwrap_err());
    let r = format!("{:?}", via_report::literal_context().unwrap_err());
    assert!(
        r.starts_with(&a),
        "anyhow's report is a prefix:\n{a}\n---\n{r}"
    );
    assert!(r[a.len()..].starts_with("\n\nLocation: "));

    // `{:#}` skips a layer whose text the previous layer already ends with.
    #[derive(Debug)]
    struct Repeats(Leaf);
    impl fmt::Display for Repeats {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "io: {}", self.0)
        }
    }
    impl StdError for Repeats {
        fn source(&self) -> Option<&(dyn StdError + 'static)> {
            Some(&self.0)
        }
    }
    let a = anyhow::Error::new(Repeats(Leaf));
    let r = report_err::Error::new(Repeats(Leaf));
    assert_eq!(format!("{a:#}"), "io: leaf failure: leaf failure");
    assert_eq!(format!("{r:#}"), "io: leaf failure");

    // The same rule skips a layer identical to the one before it, even empty.
    assert_eq!(
        format!("{:#}", via_anyhow::empty_message().unwrap_err()),
        ": "
    );
    assert_eq!(
        format!("{:#}", via_report::empty_message().unwrap_err()),
        ""
    );
}

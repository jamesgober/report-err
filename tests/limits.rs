//! `configure` sets process state once, so this binary holds one test
//! (PLAN 7.18, 12.4 E-100).

use report_err::{configure, err, limits, ConfigureError, Error, Limits};

fn with(field: &str, value: u32) -> Limits {
    let mut l = Limits::DEFAULT;
    match field {
        "max_frames" => l.max_frames = value,
        "max_attachments" => l.max_attachments = value,
        "max_sections" => l.max_sections = value,
        "max_suppressed" => l.max_suppressed = value,
        "max_tree" => l.max_tree = value,
        "max_depth" => l.max_depth = value,
        "max_bytes" => l.max_bytes = value,
        "max_chain" => l.max_chain = value,
        "max_render" => l.max_render = value,
        _ => unreachable!(),
    }
    l
}

#[test]
fn test_configure_checks_every_field_then_sets_once() {
    assert_eq!(*limits(), Limits::DEFAULT);
    assert_eq!(Limits::default(), Limits::DEFAULT);

    let ranges = [
        ("max_frames", 4, 4096),
        ("max_attachments", 0, 1024),
        ("max_sections", 0, 1024),
        ("max_suppressed", 0, 1024),
        ("max_tree", 1, 1 << 20),
        ("max_depth", 1, 64),
        ("max_bytes", 1024, 16 << 20),
        ("max_chain", 8, 4096),
        ("max_render", 4096, 64 << 20),
    ];
    // Out of range on either side: refused, naming the field; nothing set.
    for (field, min, max) in ranges {
        let mut bad = vec![max + 1];
        if min > 0 {
            bad.push(min - 1);
        }
        for value in bad {
            let refused = configure(with(field, value)).unwrap_err();
            match refused {
                ConfigureError::OutOfRange {
                    field: named,
                    value: given,
                    min: low,
                    max: high,
                } => {
                    assert_eq!(named, field);
                    assert_eq!(given, u64::from(value));
                    assert_eq!((low, high), (u64::from(min), u64::from(max)));
                }
                other => panic!("{field} = {value}: {other:?}"),
            }
            assert!(refused
                .to_string()
                .starts_with(&format!("{field} = {value} is outside {min}..={max}")));
        }
    }
    assert_eq!(*limits(), Limits::DEFAULT);

    // Two threads race; exactly one wins.
    let mut small = Limits::DEFAULT;
    small.max_frames = 4;
    small.max_chain = 8;
    small.max_suppressed = 1;
    small.max_tree = 3;
    small.max_depth = 1;
    let racers: Vec<_> = (0..2)
        .map(|_| std::thread::spawn(move || configure(small)))
        .collect();
    let results: Vec<_> = racers.into_iter().map(|r| r.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(results.contains(&Err(ConfigureError::AlreadyConfigured)));
    assert_eq!(
        configure(Limits::DEFAULT),
        Err(ConfigureError::AlreadyConfigured)
    );
    assert_eq!(limits().max_frames, 4);

    // The budgets now in force.
    let mut error = err!("root");
    for i in 0..10 {
        error = error.context(format!("layer {i}"));
    }
    assert!(format!("{error:?}").contains("Omitted: 6 context layers (budget)"));
    assert_eq!(error.chain().count(), 5);

    let deep = (0..20).fold(err!("x"), |e, i| e.context(i));
    assert!(deep.chain().len() <= 8);

    let suppressed = err!("x").suppress(err!("a")).suppress(err!("b"));
    assert_eq!(suppressed.suppressed().len(), 1);

    let group = Error::group("g", [err!("1"), err!("2"), err!("3")]);
    assert_eq!(group.children().len(), 2);
    let nested = Error::group("outer", [Error::group("inner", [err!("leaf")])]);
    assert!(nested.children().is_empty());
}

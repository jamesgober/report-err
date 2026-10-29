//! The cause chain of an error, outermost first.

use core::error::Error as StdError;
use core::iter::FusedIterator;

use crate::limits;
use crate::raw::RawError;

/// The links of an error's cause chain, outermost first: every context
/// layer, newest to oldest, then the root error and its own `source()`
/// chain. Made by [`Error::chain`](crate::Error::chain).
///
/// The context layers are read from the error's own store; the walk is
/// bounded to 128 links and stops when a `source()` returns a link it
/// already returned, so a cyclic chain cannot loop.
///
/// ```
/// use report_err::Context;
///
/// let error = std::fs::read("no/such/file").context("loading").unwrap_err();
/// let mut chain = error.chain();
/// assert_eq!(chain.len(), 2);
/// assert_eq!(chain.next().map(|l| l.to_string()).as_deref(), Some("loading"));
/// ```
#[derive(Clone)]
#[must_use = "iterators do nothing unless consumed"]
pub struct Chain<'a> {
    raw: &'a RawError,
    /// Context layers not yet yielded; the next one is `frames - 1`.
    frames: usize,
    /// The next link after the context layers.
    next: Option<&'a (dyn StdError + 'static)>,
    /// Links yielded so far.
    yielded: usize,
    /// The last two links, for the repeat check.
    recent: [Option<&'a (dyn StdError + 'static)>; 2],
    truncated: bool,
}

/// Whether `link` is one of the last two links: the same object seen as the
/// same type. An error and the first field of its struct share an address
/// but not a type, so the whole trait-object pointer is compared. The
/// compiler may emit two vtables for one type, so a repeat can go unseen;
/// the link budget still ends the walk.
fn seen(recent: &[Option<&(dyn StdError + 'static)>; 2], link: &(dyn StdError + 'static)) -> bool {
    recent.iter().flatten().any(|r| core::ptr::eq(*r, link))
}

impl<'a> Chain<'a> {
    pub(crate) fn new(raw: &'a RawError) -> Chain<'a> {
        let root: &'a (dyn StdError + 'static) = raw.root();
        Chain { raw, frames: raw.frames(), next: Some(root), yielded: 0, recent: [None, None], truncated: false }
    }

    /// Whether the walk stopped early: at the link budget, or at a link that
    /// repeated.
    pub(crate) fn truncated(&self) -> bool {
        self.truncated
    }

    fn remaining(&self) -> usize {
        let mut count = self.frames;
        let mut recent = self.recent;
        let mut cur = self.next;
        let mut yielded = self.yielded + self.frames;
        while let Some(link) = cur {
            if yielded >= limits::MAX_CHAIN || seen(&recent, link) {
                break;
            }
            recent = [recent[1], Some(link)];
            yielded += 1;
            count += 1;
            cur = link.source();
        }
        count
    }
}

impl<'a> Iterator for Chain<'a> {
    type Item = &'a (dyn StdError + 'static);

    fn next(&mut self) -> Option<Self::Item> {
        if self.yielded >= limits::MAX_CHAIN {
            if self.frames > 0 || self.next.is_some() {
                self.truncated = true;
            }
            self.frames = 0;
            self.next = None;
            return None;
        }
        if self.frames > 0 {
            self.frames -= 1;
            let frame = self.raw.frame(self.frames)?;
            self.yielded += 1;
            self.recent = [self.recent[1], Some(frame)];
            return Some(frame);
        }
        let link = self.next.take()?;
        if seen(&self.recent, link) {
            self.truncated = true;
            return None;
        }
        self.recent = [self.recent[1], Some(link)];
        self.yielded += 1;
        self.next = link.source();
        Some(link)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.remaining();
        (n, Some(n))
    }
}

impl ExactSizeIterator for Chain<'_> {}
impl FusedIterator for Chain<'_> {}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;
    use alloc::vec::Vec;
    use core::fmt;

    use crate::Error;

    #[derive(Debug)]
    struct Looping;
    impl fmt::Display for Looping {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("looping")
        }
    }
    impl core::error::Error for Looping {
        fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
            Some(self)
        }
    }

    /// A chain of `depth` links, generated without storage: each link is a
    /// distinct static.
    #[derive(Debug)]
    struct Deep(usize);
    static DEEP: [Deep; 200] = {
        let mut out = [const { Deep(0) }; 200];
        let mut i = 0;
        while i < 200 {
            out[i] = Deep(i);
            i += 1;
        }
        out
    };
    impl fmt::Display for Deep {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "deep {}", self.0)
        }
    }
    impl core::error::Error for Deep {
        fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
            DEEP.get(self.0 + 1).map(|d| d as &(dyn core::error::Error + 'static))
        }
    }

    #[test]
    fn test_chain_self_source_stops_and_is_marked_truncated() {
        // A repeat is caught when the trait-object pointers are equal; when the
        // compiler emitted two vtables for the type, the walk still stops at the
        // link budget. Either way it ends, marked truncated.
        let error = Error::new(Looping).context("ctx");
        let mut chain = error.chain();
        let texts: Vec<_> = chain.by_ref().map(|l| l.to_string()).collect();
        assert!(texts.len() >= 2 && texts.len() <= crate::limits::MAX_CHAIN, "{}", texts.len());
        assert_eq!(texts[0], "ctx");
        assert!(texts[1..].iter().all(|t| t == "looping"));
        assert!(chain.truncated());
        assert_eq!(error.chain().len(), texts.len());
        assert_eq!(error.root_cause().to_string(), "looping");
    }

    #[test]
    fn test_chain_longer_than_budget_is_bounded() {
        let error = Error::new(Deep(0));
        let mut chain = error.chain();
        assert_eq!(chain.len(), crate::limits::MAX_CHAIN);
        assert_eq!(chain.by_ref().count(), crate::limits::MAX_CHAIN);
        assert!(chain.truncated());
        assert!(chain.next().is_none());
    }

    #[test]
    fn test_chain_no_layers_is_root_then_sources() {
        let error = Error::new(Deep(198));
        let texts: Vec<_> = error.chain().map(|l| l.to_string()).collect();
        assert_eq!(texts, ["deep 198", "deep 199"]);
        assert_eq!(error.root_cause().to_string(), "deep 199");
    }

    #[derive(Debug)]
    struct Outer(Inner);
    #[derive(Debug)]
    struct Inner;
    impl fmt::Display for Outer {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("outer")
        }
    }
    impl fmt::Display for Inner {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("inner")
        }
    }
    impl core::error::Error for Inner {}
    impl core::error::Error for Outer {
        fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
            Some(&self.0)
        }
    }

    #[test]
    fn test_chain_source_at_same_address_is_not_a_repeat() {
        let error = Error::new(Outer(Inner));
        let mut chain = error.chain();
        let texts: Vec<_> = chain.by_ref().map(|l| l.to_string()).collect();
        assert_eq!(texts, ["outer", "inner"]);
        assert!(!chain.truncated());
    }

    #[test]
    fn test_chain_clone_and_exact_size() {
        let error = Error::msg("root").context("a").context("b");
        let chain = error.chain();
        let copy = chain.clone();
        assert_eq!(chain.len(), 3);
        assert_eq!(copy.count(), 3);
    }
}

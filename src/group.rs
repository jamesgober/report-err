//! Several causes and suppressed errors (PLAN 7.11, 7.12), and searching the
//! tree they make (7.7).

use alloc::vec::Vec;
use core::error::Error as StdError;
use core::fmt::{Debug, Display};
use core::marker::PhantomData;
use core::panic::Location;

use crate::error::Error;
use crate::limits::{self, MAX_DEPTH};
use crate::raw::RawError;

impl Error {
    /// Errors in this error's tree: itself, its children and suppressed
    /// errors, and theirs.
    pub(crate) fn tree_size(&self) -> u32 {
        self.raw.extras().map_or(1, |e| e.below.saturating_add(1))
    }

    /// Levels of children and suppressed errors below this one.
    pub(crate) fn height(&self) -> u32 {
        self.raw.extras().map_or(0, |e| e.height)
    }

    /// Whether `other`'s whole tree fits under this error.
    fn fits(&self, other: &Error) -> bool {
        self.tree_size().saturating_add(other.tree_size()) <= limits::max_tree()
            && other.height().saturating_add(1) <= limits::max_depth()
    }

    /// Charges `other`'s tree to this error.
    fn charge(&mut self, other: &Error) {
        let (size, height) = (other.tree_size(), other.height());
        let extras = self.raw.extras_mut();
        extras.below = extras.below.saturating_add(size);
        extras.height = extras.height.max(height.saturating_add(1));
    }

    /// Recounts the tree after children were taken out.
    fn recount(&mut self) {
        let Some(extras) = self.raw.extras() else {
            return;
        };
        let (mut below, mut height) = (0u32, 0u32);
        for e in extras.children.iter().chain(&extras.suppressed) {
            below = below.saturating_add(e.tree_size());
            height = height.max(e.height().saturating_add(1));
        }
        let extras = self.raw.extras_mut();
        extras.below = below;
        extras.height = height;
    }

    /// An error made of several failures: `message` is the root, `errors`
    /// are its children, in order.
    ///
    /// The tree is bounded (`max_tree` errors in all, `max_depth` levels); a
    /// child that does not fit is refused whole and counted under
    /// `Omitted:`.
    ///
    /// ```
    /// use report_err::{err, Error};
    ///
    /// let error = Error::group("2 of 3 shards failed", [err!("shard 1"), err!("shard 3")]);
    /// assert_eq!(error.to_string(), "2 of 3 shards failed");
    /// assert_eq!(error.children().len(), 2);
    /// ```
    #[track_caller]
    pub fn group<M, I>(message: M, errors: I) -> Error
    where
        M: Display + Debug + Send + Sync + 'static,
        I: IntoIterator<Item = Error>,
    {
        let mut group = Error::msg(message);
        for error in errors {
            group.push_child(error);
        }
        group
    }

    /// The failures this error is made of, in order. Empty for an error that
    /// is not a group.
    ///
    /// ```
    /// assert!(report_err::err!("x").children().is_empty());
    /// ```
    pub fn children(&self) -> &[Error] {
        self.raw.extras().map_or(&[], |e| e.children.as_slice())
    }

    /// Adds a child. Returns `false`, and counts it, when its tree does not
    /// fit in `max_tree` or `max_depth`.
    ///
    /// ```
    /// let mut error = report_err::err!("import failed");
    /// assert!(error.push_child(report_err::err!("row 4: bad date")));
    /// assert_eq!(error.children().len(), 1);
    /// ```
    #[track_caller]
    pub fn push_child(&mut self, child: impl Into<Error>) -> bool {
        let child = child.into();
        if !self.fits(&child) {
            let omitted = &mut self.raw.extras_mut().omitted;
            omitted.children = omitted.children.saturating_add(1);
            return false;
        }
        self.charge(&child);
        self.raw.extras_mut().children.push(child);
        true
    }

    /// Keeps `other`, a failure that happened while handling this one (a
    /// rollback that failed too). It is shown under `Suppressed:` but is not
    /// a cause: `find`, `code`, and `downcast` ignore it.
    ///
    /// Bounded by `max_suppressed`, and its tree counts against `max_tree`
    /// and `max_depth` like a child's.
    ///
    /// ```
    /// let error = report_err::err!("write failed").suppress(report_err::err!("rollback failed"));
    /// assert_eq!(error.suppressed().len(), 1);
    /// assert!(error.find::<std::fmt::Error>().is_none());
    /// ```
    #[track_caller]
    pub fn suppress(mut self, other: impl Into<Error>) -> Error {
        let other = other.into();
        let full = self.suppressed().len() >= limits::max_suppressed();
        if full || !self.fits(&other) {
            let omitted = &mut self.raw.extras_mut().omitted;
            omitted.suppressed = omitted.suppressed.saturating_add(1);
            return self;
        }
        self.charge(&other);
        self.raw.extras_mut().suppressed.push(other);
        self
    }

    /// The suppressed errors, in the order they were added.
    pub fn suppressed(&self) -> &[Error] {
        self.raw.extras().map_or(&[], |e| e.suppressed.as_slice())
    }

    /// Takes the children out, leaving the error without any.
    fn take_children(&mut self) -> Vec<Error> {
        if self.children().is_empty() {
            return Vec::new();
        }
        let children = core::mem::take(&mut self.raw.extras_mut().children);
        self.recount();
        children
    }

    /// Divides a group's children into those that match `predicate` and
    /// those that do not (Python's `ExceptionGroup.split`, on the direct
    /// children). Each half keeps the group's message, location, code,
    /// severity, exit code, kinds, and sections; an empty half is `None`.
    /// The first half is this error, so it also keeps its context layers,
    /// attachments, and suppressed errors. An error with no children is
    /// tested itself.
    ///
    /// ```
    /// use report_err::{err, Error};
    ///
    /// let error = Error::group("import failed", [err!("row 1"), err!("row 2"), err!("row 3")]);
    /// let (odd, even) = error.split(|e| e.to_string() != "row 2");
    /// assert_eq!(odd.unwrap().children().len(), 2);
    /// assert_eq!(even.unwrap().children().len(), 1);
    /// ```
    pub fn split<P>(mut self, mut predicate: P) -> (Option<Error>, Option<Error>)
    where
        P: FnMut(&Error) -> bool,
    {
        if self.children().is_empty() {
            return if predicate(&self) {
                (Some(self), None)
            } else {
                (None, Some(self))
            };
        }
        let (matching, rest): (Vec<Error>, Vec<Error>) =
            self.take_children().into_iter().partition(|e| predicate(e));
        let mut other = (!rest.is_empty()).then(|| self.same_head());
        if let Some(other) = other.as_mut() {
            for child in rest {
                other.push_child(child);
            }
        }
        if matching.is_empty() {
            return (None, other);
        }
        for child in matching {
            self.push_child(child);
        }
        (Some(self), other)
    }

    /// A new error with this one's message, location, code, overrides,
    /// kinds, and sections, and nothing else.
    fn same_head(&self) -> Error {
        use alloc::string::ToString;
        let mut head = Error {
            raw: RawError::from_message(self.to_string(), self.raw.origin()),
        };
        if let Some(code) = self.raw.code() {
            head.raw.set_code(code);
        }
        if let Some(severity) = self.raw.severity() {
            head.raw.set_severity(severity);
        }
        if let Some(exit) = self.raw.exit() {
            head.raw.set_exit(exit);
        }
        head.raw.set_flags(self.raw.flags() & crate::raw::PANIC);
        if let Some(extras) = self.raw.extras() {
            let kinds = extras.kinds;
            let sections: Vec<_> = extras
                .sections
                .iter()
                .map(|s| (s.kind, s.text.clone()))
                .collect();
            let into = head.raw.extras_mut();
            into.kinds = kinds;
            for (kind, text) in sections {
                into.add_section(kind, text);
            }
        }
        head
    }

    /// The leaves of the tree, depth first, in order. An error with no
    /// children is its own only leaf; the groups themselves are dropped.
    ///
    /// ```
    /// use report_err::{err, Error};
    ///
    /// let inner = Error::group("b", [err!("b1"), err!("b2")]);
    /// let error = Error::group("all", [err!("a"), inner, err!("c")]);
    /// let leaves: Vec<String> = error.flatten().iter().map(|e| e.to_string()).collect();
    /// assert_eq!(leaves, ["a", "b1", "b2", "c"]);
    /// ```
    #[must_use]
    pub fn flatten(self) -> Vec<Error> {
        let mut leaves = Vec::new();
        let mut stack = alloc::vec![self];
        while let Some(mut error) = stack.pop() {
            let children = error.take_children();
            if children.is_empty() {
                leaves.push(error);
            } else {
                stack.extend(children.into_iter().rev());
            }
        }
        leaves
    }

    /// The first `T` in the tree, depth first: this error's context values,
    /// root, and the root's sources, then each child's tree. Suppressed
    /// errors are not searched (see
    /// [`find_in_suppressed`](Error::find_in_suppressed)). The walk is
    /// iterative and bounded; it allocates nothing.
    ///
    /// ```
    /// use report_err::{err, Context, Error};
    ///
    /// let io = std::io::Error::from(std::io::ErrorKind::NotFound);
    /// let error = Error::group("load failed", [err!("x"), Error::new(io).context("reading")]);
    /// assert_eq!(error.find::<std::io::Error>().map(|e| e.kind()), Some(std::io::ErrorKind::NotFound));
    /// ```
    #[must_use]
    pub fn find<T: StdError + 'static>(&self) -> Option<&T> {
        self.find_all::<T>().next()
    }

    /// Every `T` in the tree, in the order [`find`](Error::find) visits them.
    ///
    /// ```
    /// use report_err::{err, Error};
    ///
    /// let io = || Error::new(std::io::Error::other("x"));
    /// let error = Error::group("all", [io(), err!("y"), io()]);
    /// assert_eq!(error.find_all::<std::io::Error>().count(), 2);
    /// ```
    pub fn find_all<T: StdError + 'static>(&self) -> FindAll<'_, T> {
        FindAll::new(core::slice::from_ref(self))
    }

    /// The first `T` among the suppressed errors' trees (each searched as
    /// [`find`](Error::find) searches), then theirs, in order.
    ///
    /// ```
    /// let error = report_err::err!("write failed")
    ///     .suppress(report_err::Error::new(std::fmt::Error));
    /// assert!(error.find::<std::fmt::Error>().is_none());
    /// assert!(error.find_in_suppressed::<std::fmt::Error>().is_some());
    /// ```
    #[must_use]
    pub fn find_in_suppressed<T: StdError + 'static>(&self) -> Option<&T> {
        let mut stack: [&[Error]; MAX_DEPTH + 1] = [&[]; MAX_DEPTH + 1];
        let mut top = 0usize;
        if let Some(slot) = stack.get_mut(0) {
            *slot = self.suppressed();
        }
        loop {
            let level = stack.get_mut(top)?;
            let Some((first, rest)) = level.split_first() else {
                top = top.checked_sub(1)?;
                continue;
            };
            *level = rest;
            if let Some(found) = first.find::<T>() {
                return Some(found);
            }
            let theirs = first.suppressed();
            if !theirs.is_empty() {
                if let Some(slot) = stack.get_mut(top + 1) {
                    *slot = theirs;
                    top += 1;
                }
            }
        }
    }
}

/// The matches of one error's own chain: context values newest first, the
/// root, then the root's sources.
struct Links<'a> {
    error: &'a Error,
    /// Context layers not yet looked at.
    frames: usize,
    root: bool,
    source: Option<&'a (dyn StdError + 'static)>,
    walked: usize,
}

impl<'a> Links<'a> {
    fn new(error: &'a Error) -> Links<'a> {
        Links {
            error,
            frames: error.raw.frames(),
            root: true,
            source: None,
            walked: 0,
        }
    }

    fn next<T: StdError + 'static>(&mut self) -> Option<&'a T> {
        let raw = &self.error.raw;
        let budget = limits::max_chain();
        if raw.has_value_frames() {
            while self.frames > 0 && self.walked < budget {
                self.frames -= 1;
                self.walked += 1;
                let value = raw
                    .frame(self.frames)
                    .and_then(|f| f.value.downcast_ref::<T>());
                if value.is_some() {
                    return value;
                }
            }
        } else {
            self.walked = self.walked.saturating_add(self.frames);
            self.frames = 0;
        }
        if core::mem::take(&mut self.root) {
            let root: &'a (dyn StdError + 'static) = raw.root();
            self.walked += 1;
            self.source = root.source();
            if let Some(found) = raw.payload_ref::<T>().or_else(|| root.downcast_ref::<T>()) {
                return Some(found);
            }
        }
        while let Some(link) = self.source.take() {
            if self.walked >= budget {
                return None;
            }
            self.walked += 1;
            self.source = link.source();
            if let Some(found) = link.downcast_ref::<T>() {
                return Some(found);
            }
        }
        None
    }
}

/// Every `T` in an error's tree. Made by [`Error::find_all`].
#[must_use = "iterators do nothing unless consumed"]
pub struct FindAll<'a, T> {
    /// The errors left at each level of the walk.
    stack: [&'a [Error]; MAX_DEPTH + 1],
    top: usize,
    current: Option<Links<'a>>,
    _type: PhantomData<fn() -> T>,
}

impl<'a, T> FindAll<'a, T> {
    fn new(start: &'a [Error]) -> FindAll<'a, T> {
        let mut stack: [&'a [Error]; MAX_DEPTH + 1] = [&[]; MAX_DEPTH + 1];
        if let Some(slot) = stack.get_mut(0) {
            *slot = start;
        }
        FindAll {
            stack,
            top: 0,
            current: None,
            _type: PhantomData,
        }
    }
}

impl<'a, T: StdError + 'static> Iterator for FindAll<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        loop {
            if let Some(links) = self.current.as_mut() {
                if let Some(found) = links.next::<T>() {
                    return Some(found);
                }
                // This error is done; walk into its children.
                let children = links.error.children();
                self.current = None;
                if !children.is_empty() {
                    if let Some(slot) = self.stack.get_mut(self.top + 1) {
                        *slot = children;
                        self.top += 1;
                    }
                }
            }
            let level = self.stack.get_mut(self.top)?;
            match level.split_first() {
                Some((first, rest)) => {
                    *level = rest;
                    self.current = Some(Links::new(first));
                }
                None => self.top = self.top.checked_sub(1)?,
            }
        }
    }
}

impl<T: StdError + 'static> core::iter::FusedIterator for FindAll<'_, T> {}

/// Gathers failures, then turns them into one error, or `Ok` when nothing
/// failed.
///
/// `len` is the exact number of failures pushed; the stored list is bounded
/// by `max_tree`, and what did not fit is counted under `Omitted:`.
///
/// ```
/// use report_err::{err, Errors, Result};
///
/// fn validate(name: &str, age: &str) -> Result<()> {
///     let mut errors = Errors::new();
///     if name.is_empty() {
///         errors.push(err!("name is empty"));
///     }
///     let age = errors.push_result(age.parse::<u8>());
///     if age.is_some_and(|a| a < 13) {
///         errors.push(err!("under 13"));
///     }
///     errors.finish("invalid sign-up form")
/// }
///
/// assert!(validate("ada", "36").is_ok());
/// let error = validate("", "x").unwrap_err();
/// assert_eq!(error.children().len(), 2);
/// ```
#[derive(Default)]
pub struct Errors {
    errors: Vec<Error>,
    /// Failures pushed in all.
    count: usize,
    /// Errors stored, counted as trees.
    stored: u32,
}

impl Errors {
    /// An empty collection. Allocates nothing until the first failure.
    #[must_use]
    pub fn new() -> Errors {
        Errors::default()
    }

    /// Adds a failure.
    #[track_caller]
    pub fn push(&mut self, error: impl Into<Error>) {
        let error = error.into();
        self.count += 1;
        let size = error.tree_size();
        // One place is kept for the group the errors become.
        if self.stored.saturating_add(size) < limits::max_tree() {
            self.stored += size;
            self.errors.push(error);
        }
    }

    /// Adds the error of a failed result and returns `None`, or returns the
    /// value.
    #[track_caller]
    pub fn push_result<T, E: Into<Error>>(&mut self, result: Result<T, E>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                self.push(error);
                None
            }
        }
    }

    /// Failures pushed, including any the budget did not keep.
    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether nothing failed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// `Ok(())`, or a group with `message` and every failure as a child.
    ///
    /// # Errors
    ///
    /// The group, when anything was pushed.
    #[track_caller]
    pub fn finish<M>(self, message: M) -> crate::Result<()>
    where
        M: Display + Debug + Send + Sync + 'static,
    {
        self.finish_with((), message)
    }

    /// `Ok(value)`, or a group with `message` and every failure as a child.
    ///
    /// # Errors
    ///
    /// The group, when anything was pushed.
    #[track_caller]
    pub fn finish_with<T, M>(self, value: T, message: M) -> crate::Result<T>
    where
        M: Display + Debug + Send + Sync + 'static,
    {
        match self.into_error(message) {
            None => Ok(value),
            Some(error) => Err(error),
        }
    }

    /// The group, or `None` when nothing was pushed.
    #[track_caller]
    pub fn into_error<M>(self, message: M) -> Option<Error>
    where
        M: Display + Debug + Send + Sync + 'static,
    {
        if self.count == 0 {
            return None;
        }
        let dropped = self.count - self.errors.len();
        let mut group = Error::group(message, self.errors);
        if dropped > 0 {
            let omitted = &mut group.raw.extras_mut().omitted;
            omitted.children = omitted
                .children
                .saturating_add(u32::try_from(dropped).unwrap_or(u32::MAX));
        }
        Some(group)
    }
}

impl Debug for Errors {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Errors")
            .field("len", &self.count)
            .field("errors", &self.errors)
            .finish()
    }
}

/// Collects every failure of an iterator of results, not only the first.
///
/// ```
/// use report_err::CollectAll;
///
/// let parsed: report_err::Result<Vec<u8>> = ["1", "x", "3", "y"].iter().map(|s| s.parse::<u8>()).collect_all();
/// let error = parsed.unwrap_err();
/// assert_eq!(error.to_string(), "2 of 4 items failed");
/// assert_eq!(error.children().len(), 2);
///
/// let fine: report_err::Result<Vec<u8>> = ["1", "2"].iter().map(|s| s.parse::<u8>()).collect_all();
/// assert_eq!(fine.unwrap(), [1, 2]);
/// ```
pub trait CollectAll<T, E>: Iterator<Item = Result<T, E>> + private::Sealed {
    /// Consumes the whole iterator: `Ok` of the collected values when every
    /// item succeeded, else a group whose message is
    /// `"{failed} of {total} items failed"` and whose children are the
    /// failures in order.
    ///
    /// # Errors
    ///
    /// The group, when any item failed.
    #[track_caller]
    fn collect_all<B: FromIterator<T>>(self) -> crate::Result<B>;
}

impl<I, T, E> CollectAll<T, E> for I
where
    I: Iterator<Item = Result<T, E>>,
    E: Into<Error>,
{
    #[track_caller]
    fn collect_all<B: FromIterator<T>>(mut self) -> crate::Result<B> {
        let location = Location::caller();
        let keep = limits::max_tree() as usize;
        let mut failed: Vec<E> = Vec::new();
        let (mut failures, mut total) = (0usize, 0usize);
        let values = B::from_iter(Successes {
            iter: &mut self,
            failed: &mut failed,
            keep,
            failures: &mut failures,
            total: &mut total,
        });
        // Whatever the collection did not ask for.
        for result in self {
            total += 1;
            if let Err(error) = result {
                failures += 1;
                if failed.len() < keep {
                    failed.push(error);
                }
            }
        }
        if failures == 0 {
            return Ok(values);
        }
        drop(values);
        // Converted here, so a converted error's origin is the caller's line.
        let mut errors = Errors::new();
        for error in failed {
            errors.push(error);
        }
        errors.count = failures;
        let message = alloc::format!("{failures} of {total} items failed");
        Err(errors.group_at(message, location))
    }
}

impl Errors {
    /// `into_error` with a given origin.
    fn group_at(
        self,
        message: alloc::string::String,
        location: &'static Location<'static>,
    ) -> Error {
        let dropped = self.count - self.errors.len();
        let mut group = Error {
            raw: RawError::from_message(message, location),
        }
        .created();
        for error in self.errors {
            group.push_child(error);
        }
        if dropped > 0 {
            let omitted = &mut group.raw.extras_mut().omitted;
            omitted.children = omitted
                .children
                .saturating_add(u32::try_from(dropped).unwrap_or(u32::MAX));
        }
        group
    }
}

/// The values of the successful items. The first `keep` failures are kept
/// unconverted; the rest are only counted.
struct Successes<'a, I, E> {
    iter: &'a mut I,
    failed: &'a mut Vec<E>,
    keep: usize,
    failures: &'a mut usize,
    total: &'a mut usize,
}

impl<I, T, E> Iterator for Successes<'_, I, E>
where
    I: Iterator<Item = Result<T, E>>,
{
    type Item = T;

    fn next(&mut self) -> Option<T> {
        for result in self.iter.by_ref() {
            *self.total += 1;
            match result {
                Ok(value) => return Some(value),
                Err(error) => {
                    *self.failures += 1;
                    if self.failed.len() < self.keep {
                        self.failed.push(error);
                    }
                }
            }
        }
        None
    }
}

mod private {
    pub trait Sealed {}
    impl<I: Iterator> Sealed for I {}
}

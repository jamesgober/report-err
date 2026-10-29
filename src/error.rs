//! `Error`: constructors, context, downcasting, and the `Deref` to
//! `dyn Error`. Safe code over `raw`.

use core::error::Error as StdError;
use core::fmt::{Debug, Display};
use core::ops::{Deref, DerefMut};
use core::panic::{Location, RefUnwindSafe, UnwindSafe};

use crate::chain::Chain;
use crate::frame::FrameValue;
use crate::limits;
use crate::raw::{DynError, RawError};

/// One error type for any error, in one machine word.
///
/// An `Error` holds any `std::error::Error + Send + Sync + 'static`, converts
/// from one through `?`, gathers context as it travels up the stack, and
/// prints a report when it reaches `main`. It is the same size as
/// `anyhow::Error`: `size_of::<Result<(), Error>>()` is one word.
///
/// It dereferences to `dyn Error`: the outermost context layer, whose
/// `source()` is the next layer inward, through every layer to the root
/// error and its own sources.
///
/// ```
/// use report_err::{Context, Result};
///
/// fn read(path: &str) -> Result<String> {
///     let text = std::fs::read_to_string(path).context("reading the settings")?;
///     Ok(text)
/// }
///
/// let error = read("no/such/file").unwrap_err();
/// assert_eq!(error.to_string(), "reading the settings");
/// assert!(error.source().is_some());                     // the io::Error
/// assert!(error.downcast_ref::<std::io::Error>().is_some());
/// ```
#[must_use]
pub struct Error {
    pub(crate) raw: RawError,
}

const _: () = assert!(core::mem::size_of::<Error>() == core::mem::size_of::<usize>());
const _: () = assert!(core::mem::size_of::<Option<Error>>() == core::mem::size_of::<usize>());
const _: () = assert!(core::mem::size_of::<Result<(), Error>>() == core::mem::size_of::<usize>());

impl UnwindSafe for Error {}
impl RefUnwindSafe for Error {}

impl Error {
    /// Wraps an error value. `?` does the same through `From`.
    ///
    /// The location of the call is recorded as the error's origin.
    ///
    /// ```
    /// let error = report_err::Error::new(std::io::Error::other("disk full"));
    /// assert_eq!(error.to_string(), "disk full");
    /// ```
    #[track_caller]
    pub fn new<E>(error: E) -> Error
    where
        E: StdError + Send + Sync + 'static,
    {
        Error { raw: RawError::from_error(error, Location::caller()) }
    }

    /// Makes an error from a message. The message stays downcastable as its
    /// own type: `&'static str` for a literal, `String` for formatted text.
    ///
    /// ```
    /// let error = report_err::Error::msg("no input files");
    /// assert_eq!(error.downcast_ref::<&str>(), Some(&"no input files"));
    /// ```
    #[track_caller]
    pub fn msg<M>(message: M) -> Error
    where
        M: Display + Debug + Send + Sync + 'static,
    {
        Error { raw: RawError::from_message(message, Location::caller()) }
    }

    /// An error whose root is a `Display`-only message (context on `None`).
    #[track_caller]
    pub(crate) fn from_display<M>(message: M) -> Error
    where
        M: Display + Send + Sync + 'static,
    {
        Error { raw: RawError::from_display(message, Location::caller()) }
    }

    /// An error value with its first context layer, in one allocation.
    #[track_caller]
    pub(crate) fn with_first_context<E, C>(error: E, context: C) -> Error
    where
        E: StdError + Send + Sync + 'static,
        C: Display + Send + Sync + 'static,
    {
        let location = Location::caller();
        let mut raw = RawError::from_error(error, location);
        raw.push_frame(location, FrameValue::new(context), limits::MAX_FRAMES);
        Error { raw }
    }

    /// Adds a layer of context: `context` becomes the outermost message and
    /// the error that was outermost becomes its source.
    ///
    /// The first layer costs no allocation; the second allocates one block,
    /// which also holds layers three to five. A `&'static str` is stored as
    /// is and a `String` is moved in. At 64 layers the oldest middle layers
    /// are dropped and counted, never the innermost or the newest.
    ///
    /// ```
    /// let error = report_err::Error::msg("disk full").context("while compacting level 2");
    /// assert_eq!(error.to_string(), "while compacting level 2");
    /// assert_eq!(format!("{error:#}"), "while compacting level 2: disk full");
    /// ```
    #[track_caller]
    pub fn context<C>(mut self, context: C) -> Error
    where
        C: Display + Send + Sync + 'static,
    {
        self.raw.push_frame(Location::caller(), FrameValue::new(context), limits::MAX_FRAMES);
        self
    }

    /// The context value or root error of type `T`, searching the context
    /// layers from the newest, then the root, as anyhow does.
    ///
    /// A message root answers to its message type; a string context layer to
    /// `&'static str` or `String`. Errors deeper in the root's own `source()`
    /// chain are not searched.
    ///
    /// ```
    /// use report_err::Context;
    ///
    /// let error = std::fs::read("no/such/file").context("loading").unwrap_err();
    /// let io = error.downcast_ref::<std::io::Error>().map(|e| e.kind());
    /// assert_eq!(io, Some(std::io::ErrorKind::NotFound));
    /// assert_eq!(error.downcast_ref::<&str>(), Some(&"loading"));
    /// ```
    pub fn downcast_ref<T>(&self) -> Option<&T>
    where
        T: Display + Debug + Send + Sync + 'static,
    {
        for index in (0..self.raw.frames()).rev() {
            if let Some(value) = self.raw.frame(index).and_then(|frame| frame.value.downcast_ref::<T>()) {
                return Some(value);
            }
        }
        self.raw.payload_ref::<T>()
    }

    /// As [`downcast_ref`](Error::downcast_ref), mutably.
    ///
    /// ```
    /// let mut error = report_err::Error::msg(String::from("draft"));
    /// if let Some(text) = error.downcast_mut::<String>() {
    ///     text.push_str(" two");
    /// }
    /// assert_eq!(error.to_string(), "draft two");
    /// ```
    pub fn downcast_mut<T>(&mut self) -> Option<&mut T>
    where
        T: Display + Debug + Send + Sync + 'static,
    {
        let mut found = None;
        for index in (0..self.raw.frames()).rev() {
            if self.raw.frame(index).is_some_and(|frame| frame.value.is::<T>()) {
                found = Some(index);
                break;
            }
        }
        match found {
            Some(index) => self.raw.frame_mut(index).and_then(|frame| frame.value.downcast_mut::<T>()),
            None => self.raw.payload_mut::<T>(),
        }
    }

    /// Moves the context value or root error of type `T` out, dropping the
    /// rest; on a miss, gives the error back unchanged.
    ///
    /// ```
    /// let error = report_err::Error::new(std::io::Error::other("gone"));
    /// let error = match error.downcast::<std::fmt::Error>() {
    ///     Ok(_) => unreachable!(),
    ///     Err(error) => error,                      // the same error
    /// };
    /// let io: std::io::Error = error.downcast().expect("the root is an io::Error");
    /// assert_eq!(io.to_string(), "gone");
    /// ```
    pub fn downcast<T>(mut self) -> Result<T, Error>
    where
        T: Display + Debug + Send + Sync + 'static,
    {
        for index in (0..self.raw.frames()).rev() {
            if self.raw.frame(index).is_some_and(|frame| frame.value.is::<T>()) {
                let value = self
                    .raw
                    .frame_mut(index)
                    .map(|frame| core::mem::replace(&mut frame.value, FrameValue::Static("")))
                    .and_then(FrameValue::take::<T>);
                return match value {
                    Some(value) => Ok(value),
                    None => Err(self),
                };
            }
        }
        self.raw.take::<T>().map_err(|raw| Error { raw })
    }

    /// Whether a context value or the root error is a `T`.
    ///
    /// ```
    /// let error = report_err::Error::msg("x");
    /// assert!(error.is::<&str>());
    /// assert!(!error.is::<String>());
    /// ```
    pub fn is<T>(&self) -> bool
    where
        T: Display + Debug + Send + Sync + 'static,
    {
        self.downcast_ref::<T>().is_some()
    }

    /// Where the error was created: the `?`, `Error::new`, or macro that made
    /// it.
    ///
    /// ```
    /// let error = report_err::err!("bad input");
    /// assert_eq!(error.location().file(), file!());
    /// ```
    pub fn location(&self) -> &'static Location<'static> {
        self.raw.origin()
    }

    /// Every link of the cause chain, outermost first: each context layer,
    /// newest to oldest, then the root error and its own `source()` chain.
    /// The same links a hand-written `source()` walk from `&*error` visits.
    ///
    /// ```
    /// use report_err::Context;
    ///
    /// let error = std::fs::read("no/such/file").context("loading").unwrap_err();
    /// let texts: Vec<String> = error.chain().map(|c| c.to_string()).collect();
    /// assert_eq!(texts.len(), 2);
    /// assert_eq!(texts[0], "loading");
    /// ```
    pub fn chain(&self) -> Chain<'_> {
        Chain::new(&self.raw)
    }

    /// The last link of the chain: the innermost cause.
    ///
    /// ```
    /// let error = report_err::Error::msg("root").context("outer");
    /// assert_eq!(error.root_cause().to_string(), "root");
    /// ```
    pub fn root_cause(&self) -> &(dyn StdError + 'static) {
        match self.chain().last() {
            Some(link) => link,
            None => self.raw.root(),
        }
    }

    /// The number of context layers dropped at the frame budget.
    pub(crate) fn omitted_frames(&self) -> u32 {
        self.raw.omitted()
    }
}

impl<E> From<E> for Error
where
    E: StdError + Send + Sync + 'static,
{
    /// What `?` calls; the location of the `?` is the error's origin.
    #[track_caller]
    #[cold]
    fn from(error: E) -> Error {
        Error::new(error)
    }
}

impl Deref for Error {
    type Target = dyn StdError + Send + Sync + 'static;

    /// The outermost layer, whose `source()` chain walks every layer and the
    /// root.
    fn deref(&self) -> &Self::Target {
        self.raw.outermost()
    }
}

impl DerefMut for Error {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.raw.outermost_mut()
    }
}

impl AsRef<DynError> for Error {
    fn as_ref(&self) -> &DynError {
        self.raw.outermost()
    }
}

impl AsRef<dyn StdError + 'static> for Error {
    fn as_ref(&self) -> &(dyn StdError + 'static) {
        self.raw.outermost()
    }
}

/// Converts any error, or an `Error` itself, into an `Error` with a context
/// layer. Sealed: implemented for every `E: Error + Send + Sync + 'static` and
/// for `Error`.
pub trait IntoErrorWithContext: crate::context::private::SealedError {
    #[doc(hidden)]
    #[track_caller]
    fn into_error_with<C>(self, context: C) -> Error
    where
        C: Display + Send + Sync + 'static;
}

impl<E> IntoErrorWithContext for E
where
    E: StdError + Send + Sync + 'static,
{
    #[track_caller]
    fn into_error_with<C>(self, context: C) -> Error
    where
        C: Display + Send + Sync + 'static,
    {
        Error::with_first_context(self, context)
    }
}

impl IntoErrorWithContext for Error {
    #[track_caller]
    fn into_error_with<C>(self, context: C) -> Error
    where
        C: Display + Send + Sync + 'static,
    {
        self.context(context)
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;
    use core::fmt;

    use super::*;

    #[derive(Debug)]
    struct Leaf;
    impl Display for Leaf {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("leaf")
        }
    }
    impl StdError for Leaf {}

    #[derive(Debug)]
    struct Wrap(Leaf);
    impl Display for Wrap {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("wrap")
        }
    }
    impl StdError for Wrap {
        fn source(&self) -> Option<&(dyn StdError + 'static)> {
            Some(&self.0)
        }
    }

    fn walk(error: &(dyn StdError + 'static)) -> Vec<String> {
        let mut out = alloc::vec![error.to_string()];
        let mut cur = error.source();
        while let Some(link) = cur {
            out.push(link.to_string());
            cur = link.source();
        }
        out
    }

    #[test]
    fn test_error_deref_source_chain_matches_chain() {
        let error = Error::new(Wrap(Leaf)).context("a").context(String::from("b")).context(7u8);
        let by_source = walk(&*error);
        let by_chain: Vec<String> = error.chain().map(|l| l.to_string()).collect();
        assert_eq!(by_source, ["7", "b", "a", "wrap", "leaf"]);
        assert_eq!(by_source, by_chain);
        let as_ref: &(dyn StdError + 'static) = error.as_ref();
        assert_eq!(walk(as_ref), by_chain);
    }

    #[test]
    fn test_error_no_context_derefs_to_root() {
        let error = Error::new(Wrap(Leaf));
        assert_eq!(error.to_string(), "wrap");
        assert_eq!(error.source().map(|s| s.to_string()).as_deref(), Some("leaf"));
        assert_eq!(error.root_cause().to_string(), "leaf");
    }

    #[test]
    fn test_error_downcast_context_newest_first() {
        let error = Error::new(Leaf).context(3u32).context(4u32);
        assert_eq!(error.downcast_ref::<u32>(), Some(&4));
        assert!(error.is::<Leaf>());
        assert!(!error.is::<Wrap>());
        let value: u32 = error.downcast().ok().unwrap_or_default();
        assert_eq!(value, 4);
    }

    #[test]
    fn test_error_downcast_miss_returns_same_error() {
        let error = Error::new(Leaf).context("ctx");
        let before = core::ptr::from_ref::<DynError>(&*error).cast::<u8>();
        let error = error.downcast::<String>().err();
        let after = error.as_ref().map(|e| core::ptr::from_ref::<DynError>(&**e).cast::<u8>());
        assert_eq!(after, Some(before));
    }

    #[test]
    fn test_error_downcast_mut_root_changes_display() {
        let mut error = Error::msg(String::from("a")).context("ctx");
        if let Some(text) = error.downcast_mut::<String>() {
            text.push('b');
        }
        assert_eq!(error.root_cause().to_string(), "ab");
    }

    #[test]
    fn test_error_msg_literal_and_string_types() {
        assert!(Error::msg("lit").is::<&str>());
        assert!(Error::msg(String::from("s")).is::<String>());
        assert!(Error::from_display(5u8).is::<u8>());
    }
}

//! The `Context` trait: context on `Result` and `Option`.

use core::convert::Infallible;
use core::fmt::Display;

use crate::error::{Error, IntoErrorWithContext};

pub(crate) mod private {
    use crate::error::Error;

    /// Seals `Context`.
    pub trait Sealed {}

    impl<T, E> Sealed for Result<T, E> where E: crate::error::IntoErrorWithContext {}
    impl<T> Sealed for Option<T> {}

    /// Seals `IntoErrorWithContext`, so the error types `Context` accepts on
    /// `Result` are exactly the standard errors and `Error`.
    pub trait SealedError {}

    impl<E> SealedError for E where E: core::error::Error + Send + Sync + 'static {}
    impl SealedError for Error {}
}

/// Context for `Result` and `Option`: say what you were doing when it failed.
///
/// Implemented for `Result<T, E>` with any `E: Error + Send + Sync +
/// 'static`, for `Result<T, report_err::Error>`, and for `Option<T>`, where
/// `None` becomes an error whose root is the context message. On `Ok` and
/// `Some` nothing runs and nothing is allocated. Every method records the
/// caller's location.
///
/// The trait is sealed, as anyhow's is.
///
/// ```
/// use report_err::{Context, Result};
///
/// fn port(text: &str) -> Result<u16> {
///     let first = text.lines().next().context("the file is empty")?;
///     let port = first.trim().parse().with_context(|| format!("bad port {first:?}"))?;
///     Ok(port)
/// }
///
/// assert_eq!(port("8080\n").unwrap(), 8080);
/// assert_eq!(port("").unwrap_err().to_string(), "the file is empty");
/// assert_eq!(port("80x").unwrap_err().to_string(), "bad port \"80x\"");
/// ```
pub trait Context<T, E>: private::Sealed {
    /// Wraps the error, or turns `None` into one, with a context message.
    ///
    /// ```
    /// use report_err::Context;
    ///
    /// let error = std::fs::read("no/such/file").context("reading the cache").unwrap_err();
    /// assert_eq!(error.to_string(), "reading the cache");
    /// ```
    #[track_caller]
    fn context<C>(self, context: C) -> Result<T, Error>
    where
        C: Display + Send + Sync + 'static;

    /// As [`context`](Context::context), with the message built only on the
    /// error path.
    ///
    /// ```
    /// use report_err::Context;
    ///
    /// let id = 42;
    /// let found: Option<&str> = None;
    /// let error = found.with_context(|| format!("no user {id}")).unwrap_err();
    /// assert_eq!(error.to_string(), "no user 42");
    /// ```
    #[track_caller]
    fn with_context<C, F>(self, f: F) -> Result<T, Error>
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C;
}

impl<T, E> Context<T, E> for Result<T, E>
where
    E: IntoErrorWithContext,
{
    #[track_caller]
    fn context<C>(self, context: C) -> Result<T, Error>
    where
        C: Display + Send + Sync + 'static,
    {
        match self {
            Ok(value) => Ok(value),
            Err(error) => Err(error.into_error_with(context)),
        }
    }

    #[track_caller]
    fn with_context<C, F>(self, f: F) -> Result<T, Error>
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        match self {
            Ok(value) => Ok(value),
            Err(error) => Err(error.into_error_with(f())),
        }
    }
}

impl<T> Context<T, Infallible> for Option<T> {
    #[track_caller]
    fn context<C>(self, context: C) -> Result<T, Error>
    where
        C: Display + Send + Sync + 'static,
    {
        match self {
            Some(value) => Ok(value),
            None => Err(Error::from_display(context)),
        }
    }

    #[track_caller]
    fn with_context<C, F>(self, f: F) -> Result<T, Error>
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        match self {
            Some(value) => Ok(value),
            None => Err(Error::from_display(f())),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;
    use core::cell::Cell;
    use core::fmt;

    use super::*;

    #[derive(Debug)]
    struct Leaf;
    impl Display for Leaf {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("leaf")
        }
    }
    impl core::error::Error for Leaf {}

    #[test]
    fn test_context_on_ok_and_some_never_calls_closure() {
        let calls = Cell::new(0);
        let ok: Result<u8, Leaf> = Ok(1);
        assert_eq!(ok.with_context(|| { calls.set(calls.get() + 1); "x" }).ok(), Some(1));
        assert_eq!(Some(2).with_context(|| { calls.set(calls.get() + 1); "x" }).ok(), Some(2));
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn test_context_on_typed_error_and_on_error() {
        let typed: Result<(), Leaf> = Err(Leaf);
        let error = typed.context("outer").unwrap_err();
        assert_eq!(error.to_string(), "outer");
        assert!(error.is::<Leaf>());
        let wrapped: Result<(), Error> = Err(error);
        let error = wrapped.context("outermost").unwrap_err();
        let texts: alloc::vec::Vec<_> = error.chain().map(|l| l.to_string()).collect();
        assert_eq!(texts, ["outermost", "outer", "leaf"]);
    }

    #[test]
    fn test_context_on_none_root_is_message() {
        let error = None::<u8>.context(7u32).unwrap_err();
        assert_eq!(error.to_string(), "7");
        assert_eq!(error.downcast_ref::<u32>(), Some(&7));
        assert!(error.source().is_none());
        assert_eq!(error.location().file(), file!());
    }
}

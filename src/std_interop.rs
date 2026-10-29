//! Moving between `Error` and `Box<dyn std::error::Error>`.

use alloc::boxed::Box;
use core::error::Error as StdError;
use core::fmt::{self, Debug, Display};
use core::panic::Location;

use crate::error::Error;
use crate::raw::{DynError, RawError};

/// An `Error` boxed as a standard error: the same `Display` and the same
/// `source()` chain.
struct Boxed(Error);

impl Display for Boxed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl Debug for Boxed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl StdError for Boxed {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        (*self.0).source()
    }
}

impl Error {
    /// The error as an owned standard error, for code that wants a
    /// `Box<dyn Error>`: the same `Display` and the same `source()` chain, in
    /// one allocation however many layers there are.
    ///
    /// ```
    /// let error = report_err::Error::msg("disk full").context("saving");
    /// let boxed: Box<dyn std::error::Error + Send + Sync> = error.into_std();
    /// assert_eq!(boxed.to_string(), "saving");
    /// assert_eq!(boxed.source().map(|s| s.to_string()).as_deref(), Some("disk full"));
    /// ```
    pub fn into_std(self) -> Box<DynError> {
        Box::new(Boxed(self))
    }

    /// An error from a boxed standard error. A box made by
    /// [`into_std`](Error::into_std) gives the original `Error` back; any
    /// other box becomes the root, which answers to
    /// `downcast_ref::<Box<dyn Error + Send + Sync>>()`, and whose concrete
    /// error is the last link of [`chain`](Error::chain).
    ///
    /// ```
    /// let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(std::fmt::Error);
    /// let error = report_err::Error::from_boxed(boxed);
    /// assert!(error.root_cause().downcast_ref::<std::fmt::Error>().is_some());
    ///
    /// let original = report_err::Error::msg("kept").context("layer");
    /// let back = report_err::Error::from_boxed(original.into_std());
    /// assert_eq!(format!("{back:#}"), "layer: kept");
    /// ```
    #[track_caller]
    pub fn from_boxed(error: Box<DynError>) -> Error {
        match error.downcast::<Boxed>() {
            Ok(boxed) => boxed.0,
            Err(error) => Error {
                raw: RawError::from_boxed(error, Location::caller()),
            },
        }
    }
}

impl From<Error> for Box<dyn StdError + Send + Sync + 'static> {
    fn from(error: Error) -> Self {
        error.into_std()
    }
}

impl From<Error> for Box<dyn StdError + Send + 'static> {
    fn from(error: Error) -> Self {
        error.into_std()
    }
}

impl From<Error> for Box<dyn StdError + 'static> {
    fn from(error: Error) -> Self {
        error.into_std()
    }
}

#[cfg(test)]
mod tests {
    use alloc::format;
    use alloc::string::ToString;

    use super::*;

    #[test]
    fn test_std_into_std_keeps_display_and_chain() {
        let error = Error::msg("root").context("a").context("b");
        let boxed: Box<dyn StdError + 'static> = error.into();
        assert_eq!(boxed.to_string(), "b");
        let mut texts = alloc::vec![boxed.to_string()];
        let mut cur = boxed.source();
        while let Some(link) = cur {
            texts.push(link.to_string());
            cur = link.source();
        }
        assert_eq!(texts, ["b", "a", "root"]);
    }

    #[test]
    fn test_std_round_trip_does_not_double_wrap() {
        let error = Error::msg("kept").context("layer");
        let at = error.location();
        let back = Error::from_boxed(error.into_std());
        assert_eq!(back.location(), at);
        assert_eq!(format!("{back:#}"), "layer: kept");
        assert_eq!(back.chain().count(), 2);
    }

    #[test]
    fn test_std_from_boxed_foreign_error() {
        let boxed: Box<DynError> = Box::new(core::fmt::Error);
        let error = Error::from_boxed(boxed);
        assert!(error.downcast_ref::<Box<DynError>>().is_some());
        assert!(error.root_cause().is::<core::fmt::Error>());
        assert!((*error).is::<core::fmt::Error>());
        let boxed: Box<DynError> = error
            .downcast()
            .unwrap_or_else(|_| Box::new(core::fmt::Error));
        assert!(boxed.is::<core::fmt::Error>());
    }
}

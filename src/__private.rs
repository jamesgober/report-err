//! What the macros expand to. Not public API: `#[doc(hidden)]`, and may
//! change in any release.

pub use alloc::format;
pub use core::result::Result::Err;
pub use core::{concat, format_args, stringify};

pub use crate::error::IntoErrorWithContext;

use core::fmt;

use crate::error::Error;

/// `err!("literal")`: a literal with no arguments is stored as
/// `&'static str`, with no `String` allocated.
#[track_caller]
#[cold]
pub fn format_err(args: fmt::Arguments<'_>) -> Error {
    match args.as_str() {
        Some(message) => Error::msg(message),
        None => Error::msg(alloc::fmt::format(args)),
    }
}

/// Chooses at compile time between an error value and a message, by
/// autoref specialisation (the stable technique anyhow uses): `TraitKind`
/// matches `(&value)` directly when `Error: From<T>`; `AdhocKind` matches
/// only after one more autoref, for any `Display + Debug` value.
pub mod kind {
    use core::fmt::{Debug, Display};

    use crate::error::Error;

    /// Marker for a message value.
    pub struct Adhoc;

    /// Selects [`Adhoc`] for any `Display + Debug` value.
    pub trait AdhocKind: Sized {
        #[inline]
        fn report_err_kind(&self) -> Adhoc {
            Adhoc
        }
    }

    impl<T> AdhocKind for &T where T: ?Sized + Display + Debug + Send + Sync + 'static {}

    impl Adhoc {
        /// A message root.
        #[track_caller]
        #[cold]
        pub fn build<M>(self, message: M) -> Error
        where
            M: Display + Debug + Send + Sync + 'static,
        {
            Error::msg(message)
        }
    }

    /// Marker for an error value.
    pub struct Trait;

    /// Selects [`Trait`] for anything `Error` converts from.
    pub trait TraitKind: Sized {
        #[inline]
        fn report_err_kind(&self) -> Trait {
            Trait
        }
    }

    impl<E> TraitKind for E where Error: From<E> {}

    /// Marker for a boxed standard error.
    pub struct Boxed;

    /// Selects [`Boxed`] for `Box<dyn Error + Send + Sync>`, so its sources
    /// stay in the chain.
    pub trait BoxedKind: Sized {
        #[inline]
        fn report_err_kind(&self) -> Boxed {
            Boxed
        }
    }

    impl BoxedKind for alloc::boxed::Box<dyn core::error::Error + Send + Sync + 'static> {}

    impl Boxed {
        /// The boxed error as the root ([`Error::from_boxed`]).
        #[track_caller]
        #[cold]
        pub fn build(
            self,
            error: alloc::boxed::Box<dyn core::error::Error + Send + Sync + 'static>,
        ) -> Error {
            Error::from_boxed(error)
        }
    }

    impl Trait {
        /// The value as the root, or an `Error` as it is.
        #[track_caller]
        #[cold]
        pub fn build<E>(self, error: E) -> Error
        where
            Error: From<E>,
        {
            Error::from(error)
        }
    }
}

//! `err!`, `format_err!`, `bail!`, `ensure!`, and the helpers they expand to.

/// Makes an [`Error`](crate::Error) from a message, a format string, or an
/// error value. anyhow's `anyhow!`.
///
/// - a string literal becomes a message root; with no `{}` arguments it is
///   stored as `&'static str`, so nothing is allocated beyond the error;
/// - a format string with arguments (or inline `{name}` captures) becomes a
///   formatted message root, a `String`;
/// - any other expression: an error value becomes the root as it is (still
///   downcastable to its type), and any other `Display + Debug` value
///   becomes a message root. The choice is made at compile time.
///
/// The location of the macro call is recorded as the error's origin.
///
/// ```
/// use report_err::err;
///
/// let a = err!("no input");
/// let port = 0;
/// let b = err!("bad port {port}");
/// let c = err!("bad port {}", port);
/// let d = err!(std::io::Error::other("disk full"));
///
/// assert_eq!(a.downcast_ref::<&str>(), Some(&"no input"));
/// assert_eq!(b.to_string(), c.to_string());
/// assert!(d.is::<std::io::Error>());
/// ```
#[macro_export]
macro_rules! err {
    ($msg:literal $(,)?) => {
        $crate::__private::format_err($crate::__private::format_args!($msg))
    };
    ($fmt:literal, $($arg:tt)*) => {
        $crate::Error::msg($crate::__private::format!($fmt, $($arg)*))
    };
    ($error:expr $(,)?) => {{
        #[allow(unused_imports)]
        use $crate::__private::kind::{AdhocKind as _, BoxedKind as _, TraitKind as _};
        match $error {
            error => (&error).report_err_kind().build(error),
        }
    }};
}

/// The same as [`err!`], under the name anyhow and eyre also export.
///
/// ```
/// let error = report_err::format_err!("code {}", 7);
/// assert_eq!(error.to_string(), "code 7");
/// ```
#[macro_export]
macro_rules! format_err {
    ($($t:tt)*) => {
        $crate::err!($($t)*)
    };
}

/// Returns early with an error: `return Err(err!(...))`.
///
/// Takes the same arguments as [`err!`].
///
/// ```
/// use report_err::{bail, Result};
///
/// fn check(port: u16) -> Result<u16> {
///     if port == 0 {
///         bail!("port {port} is not allowed");
///     }
///     Ok(port)
/// }
///
/// assert_eq!(check(0).unwrap_err().to_string(), "port 0 is not allowed");
/// ```
#[macro_export]
macro_rules! bail {
    ($($t:tt)*) => {
        return $crate::__private::Err($crate::err!($($t)*))
    };
}

/// Returns early with an error unless a condition holds.
///
/// With no message, the error says `condition failed: ` followed by the
/// condition's source text, which is printed as written, never read as a
/// format string. Otherwise it takes the same arguments as [`err!`].
///
/// ```
/// use report_err::{ensure, Result};
///
/// fn check(n: usize) -> Result<()> {
///     ensure!(n > 0);
///     ensure!(n <= 10, "batch of {n} is larger than 10");
///     Ok(())
/// }
///
/// assert_eq!(check(0).unwrap_err().to_string(), "condition failed: n > 0");
/// assert_eq!(check(11).unwrap_err().to_string(), "batch of 11 is larger than 10");
/// ```
#[macro_export]
macro_rules! ensure {
    ($cond:expr $(,)?) => {
        if !$cond {
            $crate::bail!($crate::__private::concat!("condition failed: ", $crate::__private::stringify!($cond)))
        }
    };
    ($cond:expr, $msg:literal $(,)?) => {
        if !$cond {
            $crate::bail!($msg)
        }
    };
    ($cond:expr, $fmt:literal, $($arg:tt)*) => {
        if !$cond {
            $crate::bail!($fmt, $($arg)*)
        }
    };
    ($cond:expr, $error:expr $(,)?) => {
        if !$cond {
            $crate::bail!($error)
        }
    };
}

//! Context layers: the value a `.context(...)` call stores, moved into the
//! right variant without boxing strings, and how a layer prints and
//! downcasts.

use alloc::boxed::Box;
use alloc::string::String;
use core::any::Any;
use core::fmt::{self, Debug, Display};

use crate::raw::Frame;

/// A context value of any type other than `&'static str` and `String`.
pub(crate) trait ContextValue: Display + Send + Sync + 'static {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}

impl<C: Display + Send + Sync + 'static> ContextValue for C {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

/// What one context layer holds.
pub(crate) enum FrameValue {
    /// `.context("literal")`: no allocation, no copy.
    Static(&'static str),
    /// `.context(format!(...))` or any `String`: moved in, not copied.
    Text(String),
    /// Any other value, boxed and still downcastable to its own type.
    Value(Box<dyn ContextValue>),
}

impl FrameValue {
    /// Moves `context` into the variant that fits it. After monomorphisation
    /// the two downcasts are constant branches.
    pub(crate) fn new<C: Display + Send + Sync + 'static>(context: C) -> FrameValue {
        let mut slot = Some(context);
        {
            let any: &mut dyn Any = &mut slot;
            if let Some(text) = any.downcast_mut::<Option<&'static str>>().and_then(Option::take) {
                return FrameValue::Static(text);
            }
            if let Some(text) = any.downcast_mut::<Option<String>>().and_then(Option::take) {
                return FrameValue::Text(text);
            }
        }
        match slot {
            Some(value) => FrameValue::Value(Box::new(value)),
            // The slot is only emptied by a downcast that returned above.
            None => FrameValue::Static(""),
        }
    }

    pub(crate) fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        match self {
            FrameValue::Static(text) => (text as &dyn Any).downcast_ref(),
            FrameValue::Text(text) => (text as &dyn Any).downcast_ref(),
            FrameValue::Value(value) => (**value).as_any().downcast_ref(),
        }
    }

    pub(crate) fn downcast_mut<T: 'static>(&mut self) -> Option<&mut T> {
        match self {
            FrameValue::Static(text) => (text as &mut dyn Any).downcast_mut(),
            FrameValue::Text(text) => (text as &mut dyn Any).downcast_mut(),
            FrameValue::Value(value) => (**value).as_any_mut().downcast_mut(),
        }
    }

    pub(crate) fn is<T: 'static>(&self) -> bool {
        self.downcast_ref::<T>().is_some()
    }

    /// Moves the value out when it is a `T`.
    pub(crate) fn take<T: 'static>(self) -> Option<T> {
        match self {
            FrameValue::Static(text) => {
                let mut slot = Some(text);
                (&mut slot as &mut dyn Any).downcast_mut::<Option<T>>().and_then(Option::take)
            }
            FrameValue::Text(text) => {
                let mut slot = Some(text);
                (&mut slot as &mut dyn Any).downcast_mut::<Option<T>>().and_then(Option::take)
            }
            FrameValue::Value(value) => value.into_any().downcast::<T>().ok().map(|b| *b),
        }
    }
}

impl Display for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.value {
            FrameValue::Static(text) => f.write_str(text),
            FrameValue::Text(text) => f.write_str(text),
            FrameValue::Value(value) => write!(f, "{value}"),
        }
    }
}

/// `Debug` of a layer names it, without printing the rest of the chain.
impl Debug for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Context")
            .field("message", &crate::format::Quoted(self))
            .field("location", &format_args!("{}", self.location))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use super::*;

    #[derive(Debug, PartialEq)]
    struct Shard(u32);
    impl Display for Shard {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "shard {}", self.0)
        }
    }

    #[test]
    fn test_frame_value_static_str_is_not_boxed() {
        let value = FrameValue::new("literal");
        assert!(matches!(value, FrameValue::Static("literal")));
        assert_eq!(value.downcast_ref::<&str>(), Some(&"literal"));
        assert!(value.downcast_ref::<String>().is_none());
    }

    #[test]
    fn test_frame_value_string_is_moved_not_copied() {
        let text = String::from("owned");
        let before = text.as_ptr();
        let value = FrameValue::new(text);
        match &value {
            FrameValue::Text(text) => assert_eq!(text.as_ptr(), before),
            _ => panic!("a String must be stored as Text"),
        }
        assert_eq!(value.take::<String>().as_deref(), Some("owned"));
    }

    #[test]
    fn test_frame_value_other_type_is_boxed_and_downcastable() {
        let mut value = FrameValue::new(Shard(7));
        assert!(matches!(value, FrameValue::Value(_)));
        assert!(value.is::<Shard>());
        if let Some(shard) = value.downcast_mut::<Shard>() {
            shard.0 = 8;
        }
        assert_eq!(value.take::<Shard>(), Some(Shard(8)));
    }

    #[test]
    fn test_frame_value_wrong_type_is_none() {
        assert!(FrameValue::new("a").take::<u8>().is_none());
        assert!(FrameValue::new(String::new()).take::<&str>().is_none());
        assert!(FrameValue::new(Shard(1)).take::<String>().is_none());
        assert_eq!(FrameValue::new(5u16).downcast_ref::<u16>().map(ToString::to_string), Some("5".into()));
    }
}

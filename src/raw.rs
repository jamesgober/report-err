//! The `unsafe` core: one thin pointer to a header followed by the root
//! error, with a hand-built vtable (anyhow's technique), and the flat store of
//! context layers whose `source()` links make a real cause chain.
//!
//! Nothing outside this module touches a raw pointer. Every `unsafe` block
//! names the invariant it relies on (`docs/UNSAFE.md`, I-1 to I-13). The
//! rules that keep the context store sound under both of Miri's aliasing
//! models:
//!
//! - every pointer that is kept (the node pointer, the extension pointer, a
//!   frame's link) is derived from the pointer its allocation was made with,
//!   by raw projection only, never from a reference;
//! - code reachable from `outermost`, `outermost_mut`, and a frame's
//!   `source()` creates a reference only to the one field or frame it
//!   returns, never to a whole header, node, or extension block (I-13).

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::any::TypeId;
use core::error::Error as StdError;
use core::fmt::{self, Debug, Display};
use core::mem::{ManuallyDrop, MaybeUninit};
use core::panic::Location;
use core::ptr::{self, addr_of, addr_of_mut, NonNull};

use crate::frame::FrameValue;

/// The trait object every layer of an error is seen as.
pub(crate) type DynError = dyn StdError + Send + Sync + 'static;

/// Context layers stored in the extension block before the spill vector.
const INLINE: usize = 4;

/// Context layers kept at the inner end when `max_frames` is reached.
const KEEP_INNER: usize = 8;

/// The functions that know the concrete payload type of one node.
struct VTable {
    /// Frees the whole node, payload included.
    drop: unsafe fn(NonNull<Header>),
    /// The payload as a trait object (for a boxed root, the boxed error).
    payload: unsafe fn(NonNull<Header>) -> NonNull<DynError>,
    /// Moves the payload to `out` and frees the rest of the node.
    take: unsafe fn(NonNull<Header>, NonNull<u8>),
    /// The type a caller downcasts to: `E`, or `M` for a message wrapper.
    type_id: fn() -> TypeId,
}

/// The fixed part of every node; the first field of `Node<P>` (I-1).
#[repr(C)]
pub(crate) struct Header {
    vtable: &'static VTable,
    origin: &'static Location<'static>,
    /// The extension block, made with `Box::leak` and freed in `Drop`.
    ext: Option<NonNull<Ext>>,
    /// Context layers stored: `first`, then `Ext::inline`, then `Ext::spill`.
    frames: u32,
    /// Context layers dropped at `max_frames` (saturating).
    omitted: u32,
    /// The innermost context layer; initialised when `frames >= 1`.
    first: MaybeUninit<Frame>,
}

#[repr(C)]
struct Node<P> {
    header: Header,
    payload: P,
}

/// Context layers 2 and up.
struct Ext {
    /// Layers 2 to 5; slot `i` is initialised when `frames >= i + 2`.
    inline: [MaybeUninit<Frame>; INLINE],
    /// Layers 6 and up, innermost first.
    spill: Vec<Frame>,
}

/// One context layer.
pub(crate) struct Frame {
    pub(crate) location: &'static Location<'static>,
    pub(crate) value: FrameValue,
    /// The next layer inward, or, with bit 0 set, the node, whose payload is
    /// the next link (I-12).
    link: NonNull<Frame>,
}

// SAFETY: I-12. The link is only read, and it points into the node that owns
// this frame; everything reachable through it is `Send + Sync` (I-7).
unsafe impl Send for Frame {}
// SAFETY: as for `Send`: shared access through the link only reads.
unsafe impl Sync for Frame {}

/// The owner of one node: the whole of `report_err::Error`.
pub(crate) struct RawError {
    node: NonNull<Header>,
}

// SAFETY: I-7. Every constructor requires a `Send + Sync + 'static` payload,
// context values are `Send + Sync`, and every header field is `Send + Sync`.
unsafe impl Send for RawError {}
// SAFETY: I-7, and every `&self` method only reads.
unsafe impl Sync for RawError {}

/// The payload of `Error::msg`: a message with `Display` and `Debug`.
#[repr(transparent)]
pub(crate) struct MessageError<M>(M);

/// The payload of a context on `None`: a message with `Display` only.
#[repr(transparent)]
pub(crate) struct DisplayError<M>(M);

/// The payload of `Error::from_boxed`.
#[repr(transparent)]
pub(crate) struct BoxedError(Box<DynError>);

impl<M: Display> Display for MessageError<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl<M: Debug> Debug for MessageError<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl<M: Display + Debug> StdError for MessageError<M> {}

impl<M: Display> Display for DisplayError<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl<M: Display> Debug for DisplayError<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl<M: Display> StdError for DisplayError<M> {}

impl Display for BoxedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl Debug for BoxedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl StdError for BoxedError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.0.source()
    }
}

/// # Safety
///
/// `node` is a live node made by `RawError::make::<P, _>` (I-2), not used again.
unsafe fn drop_node<P>(node: NonNull<Header>) {
    // SAFETY: I-3, I-4: the pointer came from `Box::leak` of this `Node<P>`.
    drop(unsafe { Box::from_raw(node.cast::<Node<P>>().as_ptr()) });
}

/// # Safety
///
/// `node` is a live node made by `RawError::make::<P, _>` (I-2).
unsafe fn payload_of<P: StdError + Send + Sync + 'static>(node: NonNull<Header>) -> NonNull<DynError> {
    // SAFETY: I-3, I-13: a projection to the payload field; no reference is made.
    let payload: *mut P = unsafe { addr_of_mut!((*node.cast::<Node<P>>().as_ptr()).payload) };
    let payload: *mut DynError = payload;
    NonNull::new(payload).unwrap_or_else(|| NonNull::<BoxedError>::dangling())
}

/// # Safety
///
/// `node` is a live node whose payload is a `BoxedError` (I-2).
unsafe fn payload_of_boxed(node: NonNull<Header>) -> NonNull<DynError> {
    // SAFETY: I-3: a projection to the payload field of a live node.
    let payload: *mut BoxedError = unsafe { addr_of_mut!((*node.cast::<Node<BoxedError>>().as_ptr()).payload) };
    // SAFETY: the payload is initialised; this reads only the box field, and the
    // object it points to is a separate allocation owned by the box.
    let inner: &DynError = unsafe { &*(*payload).0 };
    NonNull::from(inner)
}

/// # Safety
///
/// `node` is a live node made by `RawError::make::<P, _>`, not used again, and
/// `out` is valid for a write of a `P`.
unsafe fn take_node<P>(node: NonNull<Header>, out: NonNull<u8>) {
    let node = node.cast::<Node<P>>().as_ptr();
    // SAFETY: I-3: a projection to the payload field of a live node.
    let field = unsafe { addr_of!((*node).payload) };
    // SAFETY: I-5: the payload is initialised and read exactly once; the node
    // is freed below without dropping it again.
    let payload = unsafe { ptr::read(field) };
    // SAFETY: the caller guarantees `out` is valid for a write of a `P`.
    unsafe { out.cast::<P>().as_ptr().write(payload) };
    // SAFETY: I-4, I-5: `ManuallyDrop<P>` has the layout of `P`, so the node is
    // freed with its own layout while the payload, already moved, is not dropped.
    drop(unsafe { Box::from_raw(node.cast::<Node<ManuallyDrop<P>>>()) });
}

/// The vtable of a node with payload `P` that downcasts as `U` (I-2, I-6).
fn vtable<P: StdError + Send + Sync + 'static, U: 'static>() -> &'static VTable {
    &VTable { drop: drop_node::<P>, payload: payload_of::<P>, take: take_node::<P>, type_id: TypeId::of::<U> }
}

fn vtable_boxed() -> &'static VTable {
    &VTable {
        drop: drop_node::<BoxedError>,
        payload: payload_of_boxed,
        take: take_node::<BoxedError>,
        type_id: TypeId::of::<Box<DynError>>,
    }
}

/// The link of the innermost layer: the node pointer with bit 0 set. Headers
/// are aligned to at least 2, so the bit is free; `wrapping_byte_add` keeps
/// the node pointer's provenance.
fn node_link(node: NonNull<Header>) -> NonNull<Frame> {
    let tagged = node.as_ptr().cast::<Frame>().wrapping_byte_add(1);
    NonNull::new(tagged).unwrap_or(node.cast())
}

impl RawError {
    /// Builds a node. `U` is the type `payload_ref` answers to: `P` itself, or
    /// the type a `#[repr(transparent)]` wrapper `P` holds (I-6).
    fn make<P: StdError + Send + Sync + 'static>(
        payload: P,
        vtable: &'static VTable,
        origin: &'static Location<'static>,
    ) -> RawError {
        const { assert!(core::mem::offset_of!(Node<P>, header) == 0) };
        let header =
            Header { vtable, origin, ext: None, frames: 0, omitted: 0, first: MaybeUninit::uninit() };
        let node: &mut Node<P> = Box::leak(Box::new(Node { header, payload }));
        RawError { node: NonNull::from(node).cast() }
    }

    pub(crate) fn from_error<E>(error: E, origin: &'static Location<'static>) -> RawError
    where
        E: StdError + Send + Sync + 'static,
    {
        RawError::make(error, vtable::<E, E>(), origin)
    }

    pub(crate) fn from_message<M>(message: M, origin: &'static Location<'static>) -> RawError
    where
        M: Display + Debug + Send + Sync + 'static,
    {
        RawError::make(MessageError(message), vtable::<MessageError<M>, M>(), origin)
    }

    pub(crate) fn from_display<M>(message: M, origin: &'static Location<'static>) -> RawError
    where
        M: Display + Send + Sync + 'static,
    {
        RawError::make(DisplayError(message), vtable::<DisplayError<M>, M>(), origin)
    }

    pub(crate) fn from_boxed(error: Box<DynError>, origin: &'static Location<'static>) -> RawError {
        RawError::make(BoxedError(error), vtable_boxed(), origin)
    }

    fn header(&self) -> &Header {
        // SAFETY: I-3, I-8: the node is live while `self` is; shared access.
        unsafe { self.node.as_ref() }
    }

    fn vtable(&self) -> &'static VTable {
        self.header().vtable
    }

    pub(crate) fn origin(&self) -> &'static Location<'static> {
        self.header().origin
    }

    pub(crate) fn frames(&self) -> usize {
        self.header().frames as usize
    }

    pub(crate) fn omitted(&self) -> u32 {
        self.header().omitted
    }

    /// The type `payload_ref` answers to.
    pub(crate) fn root_type_id(&self) -> TypeId {
        (self.vtable().type_id)()
    }

    /// The root error, as the next link after the innermost layer.
    pub(crate) fn root(&self) -> &DynError {
        // SAFETY: I-2: the vtable belongs to this node.
        let payload = unsafe { (self.vtable().payload)(self.node) };
        // SAFETY: I-3: the payload is live and shared for as long as `self` is.
        unsafe { payload.as_ref() }
    }

    fn root_mut(&mut self) -> &mut DynError {
        // SAFETY: I-2: the vtable belongs to this node.
        let mut payload = unsafe { (self.vtable().payload)(self.node) };
        // SAFETY: I-3, I-8: unique access through `&mut self`.
        unsafe { payload.as_mut() }
    }

    pub(crate) fn payload_ref<T: 'static>(&self) -> Option<&T> {
        if self.root_type_id() != TypeId::of::<T>() {
            return None;
        }
        let node = self.node.as_ptr().cast::<Node<T>>();
        // SAFETY: I-3: a projection to the payload field of a live node; `Node`
        // is `repr(C)`, so the payload sits where it does in `Node<P>` when `P`
        // is a transparent wrapper of `T` (I-6).
        let field = unsafe { addr_of!((*node).payload) };
        // SAFETY: I-6: the payload is a `T` or a transparent wrapper of one.
        Some(unsafe { &*field })
    }

    pub(crate) fn payload_mut<T: 'static>(&mut self) -> Option<&mut T> {
        if self.root_type_id() != TypeId::of::<T>() {
            return None;
        }
        let node = self.node.as_ptr().cast::<Node<T>>();
        // SAFETY: I-3, I-6: as in `payload_ref`.
        let field = unsafe { addr_of_mut!((*node).payload) };
        // SAFETY: I-6, I-8: as in `payload_ref`, with unique access.
        Some(unsafe { &mut *field })
    }

    /// Moves the root out when it is a `T`; otherwise gives the error back.
    pub(crate) fn take<T: 'static>(self) -> Result<T, RawError> {
        if self.root_type_id() != TypeId::of::<T>() {
            return Err(self);
        }
        let this = ManuallyDrop::new(self);
        let mut out = MaybeUninit::<T>::uninit();
        let out_ptr = NonNull::from(&mut out).cast::<u8>();
        // SAFETY: I-5, I-6: the payload is a `T` (or a transparent wrapper of
        // one), `out` has room for it, and `this` is never dropped or used again.
        unsafe { (this.vtable().take)(this.node, out_ptr) };
        // SAFETY: `take` wrote a `T` to `out`.
        Ok(unsafe { out.assume_init() })
    }

    /// Context layer `index` (0 is the innermost), if it exists.
    pub(crate) fn frame(&self, index: usize) -> Option<&Frame> {
        if index >= self.frames() {
            return None;
        }
        let frame = frame_ptr(self.node, index);
        // SAFETY: I-12, I-13: `index` is below `frames`, so the slot is
        // initialised; the reference covers that frame only.
        NonNull::new(frame).map(|f| unsafe { &*f.as_ptr() })
    }

    pub(crate) fn frame_mut(&mut self, index: usize) -> Option<&mut Frame> {
        if index >= self.frames() {
            return None;
        }
        let frame = frame_ptr(self.node, index);
        // SAFETY: as in `frame`, with unique access through `&mut self`.
        NonNull::new(frame).map(|f| unsafe { &mut *f.as_ptr() })
    }

    /// The outermost layer: the newest context, or the root.
    pub(crate) fn outermost(&self) -> &DynError {
        match self.frames().checked_sub(1).and_then(|i| self.frame(i)) {
            Some(frame) => frame,
            None => self.root(),
        }
    }

    pub(crate) fn outermost_mut(&mut self) -> &mut DynError {
        if let Some(index) = self.frames().checked_sub(1) {
            if let Some(frame) = NonNull::new(frame_ptr(self.node, index)) {
                // SAFETY: I-12, I-13: the outermost frame is initialised; the
                // reference covers that frame only, with unique access.
                return unsafe { &mut *frame.as_ptr() };
            }
        }
        self.root_mut()
    }

    /// Adds `value` as the new outermost layer. At `max_frames`, the oldest
    /// layer above the innermost `KEEP_INNER` is dropped first and counted.
    pub(crate) fn push_frame(
        &mut self,
        location: &'static Location<'static>,
        value: FrameValue,
        max_frames: usize,
    ) {
        let max_frames = max_frames.max(2);
        if self.frames() >= max_frames {
            let index = KEEP_INNER.min(self.frames() - 1);
            self.remove_frame(index);
            let header = self.header_mut();
            header.omitted = header.omitted.saturating_add(1);
        }
        let index = self.frames();
        let link = if index == 0 { node_link(self.node) } else { frame_link(self.node, index - 1) };
        let frame = Frame { location, value, link };
        if index == 0 {
            // SAFETY: I-3: a projection to the `first` slot, which is not
            // initialised (`frames` is 0).
            let slot = unsafe { addr_of_mut!((*self.node.as_ptr()).first) };
            // SAFETY: the slot is valid for writes and holds no value to drop.
            unsafe { slot.write(MaybeUninit::new(frame)) };
        } else {
            let ext = self.ext_or_alloc();
            if index - 1 < INLINE {
                // SAFETY: I-3: a projection to the inline slots of a live block.
                let inline = unsafe { addr_of_mut!((*ext.as_ptr()).inline) };
                let slot = inline.cast::<MaybeUninit<Frame>>().wrapping_add(index - 1);
                // SAFETY: the slot is inside the array and not initialised.
                unsafe { slot.write(MaybeUninit::new(frame)) };
            } else {
                let spill = spill_mut(ext.as_ptr());
                let before = spill.as_ptr();
                spill.push(frame);
                let moved = spill.as_ptr() != before;
                let header = self.header_mut();
                header.frames = header.frames.saturating_add(1);
                if moved {
                    // The vector reallocated: every spilled layer moved,
                    // including the one just pushed, whose link pointed into
                    // the old buffer.
                    self.relink_from(1 + INLINE);
                }
                return;
            }
        }
        let header = self.header_mut();
        header.frames = header.frames.saturating_add(1);
    }

    fn header_mut(&mut self) -> &mut Header {
        // SAFETY: I-8: unique access through `&mut self`.
        unsafe { self.node.as_mut() }
    }

    fn ext_or_alloc(&mut self) -> NonNull<Ext> {
        if let Some(ext) = self.header().ext {
            return ext;
        }
        let block = Ext {
            inline: [MaybeUninit::uninit(), MaybeUninit::uninit(), MaybeUninit::uninit(), MaybeUninit::uninit()],
            spill: Vec::new(),
        };
        let ext = NonNull::from(Box::leak(Box::new(block)));
        self.header_mut().ext = Some(ext);
        ext
    }

    /// Removes layer `index` (never the innermost), keeps the rest in order,
    /// and rewrites the links that moved.
    fn remove_frame(&mut self, index: usize) {
        let count = self.frames();
        if index == 0 || index >= count {
            return;
        }
        let Some(ext) = self.header().ext else { return };
        let ext = ext.as_ptr();
        if index - 1 < INLINE {
            // SAFETY: I-3: a projection to the inline slots.
            let inline = unsafe { addr_of_mut!((*ext).inline) }.cast::<MaybeUninit<Frame>>();
            let slot = inline.wrapping_add(index - 1);
            // SAFETY: the slot is initialised (`index < frames`); its frame is
            // dropped here and the slot refilled below.
            unsafe { ptr::drop_in_place(slot.cast::<Frame>()) };
            let used = (count - 1).min(INLINE);
            let after = used - index;
            // SAFETY: shifts the initialised slots after `slot` down by one,
            // inside the array.
            unsafe { ptr::copy(slot.wrapping_add(1), slot, after) };
            let spill = spill_mut(ext);
            if !spill.is_empty() {
                let moved = spill.remove(0);
                // SAFETY: the last inline slot was vacated by the shift.
                unsafe { inline.wrapping_add(INLINE - 1).write(MaybeUninit::new(moved)) };
            }
        } else {
            let spill = spill_mut(ext);
            let at = index - 1 - INLINE;
            if at < spill.len() {
                drop(spill.remove(at));
            }
        }
        let header = self.header_mut();
        header.frames = header.frames.saturating_sub(1);
        self.relink_from(index);
    }

    /// Rewrites the link of every layer from `start` outward.
    fn relink_from(&mut self, start: usize) {
        for index in start.max(1)..self.frames() {
            let link = frame_link(self.node, index - 1);
            let frame = frame_ptr(self.node, index);
            if frame.is_null() {
                return;
            }
            // SAFETY: I-3: a projection to the link field of an initialised frame.
            let field = unsafe { addr_of_mut!((*frame).link) };
            // SAFETY: I-12: `index < frames`, so the frame is initialised; only
            // its link is written.
            unsafe { field.write(link) };
        }
    }
}

/// A pointer to layer `index`, derived from the node pointer by projection
/// only (I-3, I-13), or null when the layer's block is missing. The caller
/// checks `index < frames`.
fn frame_ptr(node: NonNull<Header>, index: usize) -> *mut Frame {
    let node = node.as_ptr();
    if index == 0 {
        // SAFETY: I-3: a projection to the `first` slot of a live node.
        return unsafe { addr_of_mut!((*node).first) }.cast();
    }
    // SAFETY: I-3: a projection to the `ext` field of a live node.
    let field = unsafe { addr_of!((*node).ext) };
    // SAFETY: the field is initialised; it is read by value (a copy).
    let Some(ext) = (unsafe { *field }) else { return ptr::null_mut() };
    let ext = ext.as_ptr();
    if index - 1 < INLINE {
        // SAFETY: I-3: a projection to the inline slots of a live block.
        let inline = unsafe { addr_of_mut!((*ext).inline) };
        return inline.cast::<Frame>().wrapping_add(index - 1);
    }
    // SAFETY: I-3: a projection to the `spill` field of a live block.
    let field = unsafe { addr_of!((*ext).spill) };
    // SAFETY: I-13: a shared reference to the vector itself, not its
    // elements; `as_ptr` makes no reference to the elements.
    let spill = unsafe { &*field };
    spill.as_ptr().cast_mut().wrapping_add(index - 1 - INLINE)
}

/// The spill vector of a live block, for a caller that holds the node
/// uniquely (`&mut RawError`).
fn spill_mut<'a>(ext: *mut Ext) -> &'a mut Vec<Frame> {
    // SAFETY: I-3: a projection to the `spill` field of a live block.
    let field = unsafe { addr_of_mut!((*ext).spill) };
    // SAFETY: I-8, I-13: the caller has unique access to the node; the
    // reference covers the vector itself, not its elements, and ends before
    // the caller's borrow does.
    unsafe { &mut *field }
}

fn frame_link(node: NonNull<Header>, index: usize) -> NonNull<Frame> {
    NonNull::new(frame_ptr(node, index)).unwrap_or_else(|| node_link(node))
}

impl Drop for RawError {
    fn drop(&mut self) {
        let drop_fn = self.vtable().drop;
        // SAFETY: I-2, I-4: the vtable belongs to this node, which is freed once.
        unsafe { drop_fn(self.node) }
    }
}

impl Drop for Header {
    fn drop(&mut self) {
        let count = self.frames as usize;
        if count >= 1 {
            // SAFETY: `frames >= 1`, so `first` is initialised; dropped once.
            unsafe { self.first.assume_init_drop() };
        }
        if let Some(ext) = self.ext.take() {
            // SAFETY: I-4: the block came from `Box::leak` and is freed once.
            let mut block = unsafe { Box::from_raw(ext.as_ptr()) };
            let used = count.saturating_sub(1).min(INLINE);
            for slot in block.inline.iter_mut().take(used) {
                // SAFETY: the first `used` inline slots are initialised.
                unsafe { slot.assume_init_drop() };
            }
            // The spill vector drops its own frames with the box.
        }
    }
}

impl StdError for Frame {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        let link = self.link.as_ptr();
        if (link as usize) & 1 == 1 {
            let node = link.wrapping_byte_sub(1).cast::<Header>();
            let node = NonNull::new(node)?;
            // SAFETY: I-12: the link of the innermost layer is its own node,
            // live while this frame is; a projection to the vtable field.
            let field = unsafe { addr_of!((*node.as_ptr()).vtable) };
            // SAFETY: the field is initialised; it is read by value (a copy).
            let vtable = unsafe { *field };
            // SAFETY: I-2: the vtable belongs to this node.
            let payload = unsafe { (vtable.payload)(node) };
            // SAFETY: I-13: a reference to the payload only, live as `self` is.
            return Some(unsafe { payload.as_ref() });
        }
        // SAFETY: I-12, I-13: the link is the next frame inward, initialised and
        // owned by the same node; the reference covers that frame only.
        Some(unsafe { &*link })
    }
}

// The layout the design depends on (PLAN 7.2, 7.6.1).
const _: () = assert!(core::mem::size_of::<Frame>() == 5 * core::mem::size_of::<usize>());
const _: () = assert!(core::mem::align_of::<Header>() >= 2);

#[cfg(test)]
mod tests {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec;
    use core::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[derive(Debug)]
    struct Root(&'static str);
    impl Display for Root {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.0)
        }
    }
    impl StdError for Root {}

    fn here() -> &'static Location<'static> {
        Location::caller()
    }

    fn build(layers: usize, max: usize) -> RawError {
        let mut raw = RawError::from_error(Root("root"), here());
        for i in 0..layers {
            raw.push_frame(here(), FrameValue::new(format!("layer {i}")), max);
        }
        raw
    }

    fn walk(raw: &RawError) -> vec::Vec<String> {
        let mut out = vec![raw.outermost().to_string()];
        let mut cur = raw.outermost().source();
        while let Some(link) = cur {
            out.push(link.to_string());
            cur = link.source();
        }
        out
    }

    fn expected(layers: impl Iterator<Item = usize>) -> vec::Vec<String> {
        let mut v: vec::Vec<String> = layers.map(|i| format!("layer {i}")).collect();
        v.push("root".to_string());
        v
    }

    #[test]
    fn test_raw_header_offset_is_zero() {
        assert_eq!(core::mem::offset_of!(Node<Root>, header), 0);
        assert_eq!(core::mem::offset_of!(Node<[u8; 3]>, header), 0);
    }

    #[test]
    fn test_raw_frame_source_walks_every_layer() {
        for layers in [0, 1, 2, 4, 5, 6, 9, 20] {
            let raw = build(layers, 64);
            assert_eq!(walk(&raw), expected((0..layers).rev()), "layers = {layers}");
        }
    }

    #[test]
    fn test_raw_max_frames_drops_oldest_middle_layer() {
        let raw = build(12, 10);
        assert_eq!(raw.frames(), 10);
        assert_eq!(raw.omitted(), 2);
        // Kept: the innermost 8 (0 to 7) and the two newest (10, 11).
        let mut want: vec::Vec<usize> = (0..8).collect();
        want.extend([10, 11]);
        want.reverse();
        assert_eq!(walk(&raw), expected(want.into_iter()));
    }

    #[test]
    fn test_raw_remove_inline_layer_pulls_spill_frame_in() {
        let mut raw = build(8, 64);
        raw.remove_frame(2);
        let want = [7, 6, 5, 4, 3, 1, 0];
        assert_eq!(walk(&raw), expected(want.into_iter()));
        raw.remove_frame(1);
        assert_eq!(walk(&raw), expected([7, 6, 5, 4, 3, 0].into_iter()));
    }

    #[test]
    fn test_raw_spill_growth_keeps_links() {
        let raw = build(100, 128);
        assert_eq!(walk(&raw), expected((0..100).rev()));
    }

    #[test]
    fn test_raw_deref_mut_then_source_then_reuse() {
        for layers in [1, 3, 5, 9] {
            let mut raw = build(layers, 64);
            let top: &mut DynError = raw.outermost_mut();
            let below = top.source().map(|s| s.to_string());
            assert!(below.is_some());
            assert!(top.downcast_mut::<Root>().is_none());
            assert_eq!(top.to_string(), format!("layer {}", layers - 1));
        }
        let mut raw = build(0, 64);
        assert!(raw.outermost_mut().downcast_mut::<Root>().is_some());
    }

    #[test]
    fn test_raw_payload_ref_wrong_type_is_none() {
        let raw = RawError::from_message("text", here());
        assert_eq!(raw.payload_ref::<&str>(), Some(&"text"));
        assert!(raw.payload_ref::<String>().is_none());
        assert!(raw.payload_ref::<MessageError<&str>>().is_none());
        let raw = RawError::from_display(7u8, here());
        assert_eq!(raw.payload_ref::<u8>(), Some(&7));
        let raw = RawError::from_error(Root("r"), here());
        assert!(raw.payload_ref::<Root>().is_some());
        assert!(raw.payload_ref::<u8>().is_none());
    }

    static DROPS: AtomicUsize = AtomicUsize::new(0);

    #[derive(Debug)]
    struct Counted(u32);
    impl Display for Counted {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "counted {}", self.0)
        }
    }
    impl StdError for Counted {}
    impl Drop for Counted {
        fn drop(&mut self) {
            DROPS.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn test_raw_drop_runs_payload_drop_once_and_take_moves_payload() {
        DROPS.store(0, Ordering::SeqCst);
        let mut raw = RawError::from_error(Counted(1), here());
        raw.push_frame(here(), FrameValue::new("a"), 64);
        raw.push_frame(here(), FrameValue::new(String::from("b")), 64);
        drop(raw);
        assert_eq!(DROPS.load(Ordering::SeqCst), 1);

        let mut raw = RawError::from_error(Counted(2), here());
        raw.push_frame(here(), FrameValue::new("a"), 64);
        let raw = match raw.take::<u8>() {
            Ok(_) => panic!("wrong type taken"),
            Err(raw) => raw,
        };
        let taken = raw.take::<Counted>().ok().map(|c| c.0);
        assert_eq!(taken, Some(2));
        assert_eq!(DROPS.load(Ordering::SeqCst), 2);
    }

    #[derive(Debug)]
    struct Zst;
    impl Display for Zst {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("zst")
        }
    }
    impl StdError for Zst {}

    #[test]
    fn test_raw_zero_sized_payload() {
        let mut raw = RawError::from_error(Zst, here());
        raw.push_frame(here(), FrameValue::new("ctx"), 64);
        assert_eq!(walk(&raw), ["ctx", "zst"]);
        assert!(raw.take::<Zst>().is_ok());
    }

    #[derive(Debug)]
    #[repr(align(64))]
    struct Align64(u8);
    impl Display for Align64 {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "a64 {}", self.0)
        }
    }
    impl StdError for Align64 {}

    #[derive(Debug)]
    #[repr(align(4096))]
    struct Align4096(u8);
    impl Display for Align4096 {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "a4096 {}", self.0)
        }
    }
    impl StdError for Align4096 {}

    #[test]
    fn test_raw_over_aligned_payload_64() {
        let raw = RawError::from_error(Align64(9), here());
        let p = raw.payload_ref::<Align64>().map(|p| core::ptr::from_ref(p) as usize);
        assert_eq!(p.map(|p| p % 64), Some(0));
        assert_eq!(raw.root().to_string(), "a64 9");
    }

    #[test]
    fn test_raw_over_aligned_payload_4096() {
        let mut raw = RawError::from_error(Align4096(3), here());
        raw.push_frame(here(), FrameValue::new("ctx"), 64);
        let p = raw.payload_ref::<Align4096>().map(|p| core::ptr::from_ref(p) as usize);
        assert_eq!(p.map(|p| p % 4096), Some(0));
        assert_eq!(walk(&raw), ["ctx", "a4096 3"]);
    }

    #[test]
    fn test_raw_boxed_root_is_the_inner_error() {
        let inner: Box<DynError> = Box::new(Root("boxed"));
        let raw = RawError::from_boxed(inner, here());
        assert!(raw.root().downcast_ref::<Root>().is_some());
        assert!(raw.payload_ref::<Box<DynError>>().is_some());
        let b = raw.take::<Box<DynError>>().ok();
        assert!(b.is_some_and(|b| b.downcast_ref::<Root>().is_some()));
    }
}

//! iOS software-keyboard shim.
//!
//! winit's own `WinitUIView` conforms to `UIKeyInput`, so `set_ime_allowed(true)`
//! already raises the iOS keyboard and delivers ASCII via `WindowEvent::Keyboard-
//! Input`. But `UIKeyInput` has no marked-text support, so Japanese (kana/kanji
//! composition) never reaches the app — a dealbreaker for a Japanese-text app.
//!
//! This module attaches a hidden `UITextField` (a full `UITextInput`) to the
//! winit view instead. Making it first responder establishes a real text-input
//! session — iOS raises the keyboard automatically AND drives marked-text
//! composition, so Japanese works. We observe the field's `editingChanged`
//! event, diff the whole text against the previous value, and enqueue the delta
//! as `Text` / `Backspace` for the runtime to drain each frame and route exactly
//! like a physical key.
//!
//! IMPORTANT: this only delivers input because the declarative runtime parks the
//! iOS event loop with `ControlFlow::Wait` when idle (see `declarative.rs`). A
//! constant timer wakeup starves UIKit's text-input run-loop sources, and then
//! the keyboard shows but `insertText:` / `editingChanged` never fire — for this
//! field OR for winit's own view.
//!
//! Everything is main-thread only (winit-iOS and UIKit callbacks both run on
//! the main thread), so the `thread_local` queue needs no locking.

#![cfg(target_os = "ios")]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_foundation::{NSObject, NSObjectProtocol};
use objc2_ui_kit::{UIControlEvents, UITextField, UIView};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// A text-input event, drained by the scene runtime each frame.
#[derive(Debug)]
pub enum KbEvent {
    /// Text inserted since the last change (a committed IME candidate may be
    /// several chars). "\n" would mean the return key, but the field is
    /// single-line so returns arrive via a separate control event we ignore.
    Text(String),
    /// One character deleted.
    Backspace,
}

thread_local! {
    /// Deltas derived from `editingChanged`, drained per frame.
    static QUEUE: RefCell<VecDeque<KbEvent>> = const { RefCell::new(VecDeque::new()) };
    /// The attached shim (created once, on the first frame after the window exists).
    static SHIM: RefCell<Option<Shim>> = const { RefCell::new(None) };
    /// Last full text seen, so we can diff into insert/delete deltas.
    static LAST_TEXT: RefCell<String> = const { RefCell::new(String::new()) };
}

struct Shim {
    field: Retained<UITextField>,
    /// Kept alive: the field holds only a weak reference to its action target.
    _target: Retained<KbTarget>,
    /// Deduped first-responder state so we only become/resign on a transition.
    active: bool,
    /// The winit window, so the `editingChanged` callback can request a redraw —
    /// UIKit edits bypass winit, so without this the runtime's lazy_render loop
    /// stays parked and the typed text is never drained.
    window: Arc<Window>,
}

/// Wake the runtime so the just-queued delta gets drained + rendered this frame.
fn request_redraw() {
    SHIM.with(|slot| {
        if let Some(shim) = slot.borrow().as_ref() {
            shim.window.request_redraw();
        }
    });
}

/// Diff `new` against the last seen text and enqueue the change as backspaces
/// (for the removed tail) followed by an insert (for the added tail). Handles
/// IME commits, which replace a marked region with the chosen text in one edit.
fn push_delta(new: &str) {
    LAST_TEXT.with(|lt| {
        let old = lt.borrow().clone();
        let old_chars: Vec<char> = old.chars().collect();
        let new_chars: Vec<char> = new.chars().collect();
        let common = old_chars
            .iter()
            .zip(new_chars.iter())
            .take_while(|(a, b)| a == b)
            .count();
        QUEUE.with(|q| {
            let mut q = q.borrow_mut();
            for _ in common..old_chars.len() {
                q.push_back(KbEvent::Backspace);
            }
            if common < new_chars.len() {
                let added: String = new_chars[common..].iter().collect();
                q.push_back(KbEvent::Text(added));
            }
        });
        *lt.borrow_mut() = new.to_string();
    });
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SabitoriKbTarget"]
    struct KbTarget;

    unsafe impl NSObjectProtocol for KbTarget {}

    impl KbTarget {
        // UIControlEventEditingChanged action: fires whenever the field's text
        // changes (including on IME commit), giving us the full current value.
        #[unsafe(method(textChanged:))]
        fn text_changed(&self, sender: &UITextField) {
            let new = sender.text().map(|s| s.to_string()).unwrap_or_default();
            push_delta(&new);
            request_redraw();
        }
    }
);

impl KbTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

/// Attach the hidden text field to the winit `UIView`, once. Idempotent — safe
/// to call every frame. No-op off the main thread or before the window has a
/// UIKit handle.
pub fn ensure_attached(window: &Arc<Window>) {
    SHIM.with(|slot| {
        if slot.borrow().is_some() {
            return;
        }
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let ui_view_ptr = match handle.as_raw() {
            RawWindowHandle::UiKit(h) => h.ui_view.as_ptr(),
            _ => return,
        };
        // Zero-frame, never drawn — exists only to own the keyboard session.
        let field: Retained<UITextField> = unsafe { msg_send![UITextField::alloc(mtm), init] };
        let target = KbTarget::new(mtm);
        let target_obj: &AnyObject = &target;
        unsafe {
            field.addTarget_action_forControlEvents(
                Some(target_obj),
                sel!(textChanged:),
                UIControlEvents::EditingChanged,
            );
            let host: &UIView = &*(ui_view_ptr as *const UIView);
            host.addSubview(&field);
        }
        *slot.borrow_mut() = Some(Shim {
            field,
            _target: target,
            active: false,
            window: Arc::clone(window),
        });
    });
}

/// Match the software keyboard to focus: raise it when a field is focused,
/// dismiss it otherwise. Deduped, so it only acts on a transition.
pub fn set_active(active: bool) {
    SHIM.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(shim) = slot.as_mut() else {
            return;
        };
        if shim.active == active {
            return;
        }
        shim.active = active;
        if active {
            let _: bool = shim.field.becomeFirstResponder();
        } else {
            let _: bool = shim.field.resignFirstResponder();
            // Reset the field + diff baseline so the next focus starts clean.
            shim.field.setText(None);
            LAST_TEXT.with(|lt| lt.borrow_mut().clear());
        }
    });
}

/// Drain queued text deltas (FIFO). Collected into a `Vec` first so the
/// runtime's routing can't re-enter the borrow.
pub fn drain() -> Vec<KbEvent> {
    QUEUE.with(|q| q.borrow_mut().drain(..).collect())
}

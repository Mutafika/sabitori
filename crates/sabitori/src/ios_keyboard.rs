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
//! event, split the text at the composition (UIKit's *marked text*), diff the
//! committed part against its previous value, and enqueue the delta as `Text`
//! / `Backspace` for the runtime to drain each frame and route exactly like a
//! physical key. The composition goes out as an IME preedit and what it turns
//! into as an IME commit — what winit sends on a desktop — so the app draws
//! the unconfirmed text as such and never gets it as typed.
//!
//! Keys that change no text never show up in that diff: Esc, the arrows, Tab,
//! Ctrl+C on a hardware keyboard, and Backspace in an empty field. The field is
//! our own subclass ([`KbField`]) so it can take those itself — the hardware
//! ones from `pressesBegan:` before the field would use them (an arrow would
//! only move its invisible caret), Backspace from `deleteBackward` — and hand
//! them on as keys ([`KbEvent::Key`]), the way a desktop keyboard does.
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

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::sync::Arc;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, ClassType, MainThreadMarker, MainThreadOnly};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{NSArray, NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol, NSRange, NSSet, NSString};
use objc2_ui_kit::{
    UIControl, UIControlEvents, UIKey, UIKeyModifierFlags, UIPress, UIPressesEvent, UIResponder,
    UITextAutocorrectionType, UITextField, UITextInput, UITextInputCurrentInputModeDidChangeNotification,
    UITextInputTraits, UITextPosition, UIView,
};
use sabitori_input::{Key, Modifiers};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// A text-input event, drained by the scene runtime each frame.
#[derive(Debug)]
pub enum KbEvent {
    /// Text typed since the last change, outside any composition. The field
    /// is single-line, so Return never shows up here — it arrives as
    /// [`KbEvent::Enter`].
    Text(String),
    /// The composition in progress (`InputEvent::ImePreedit`): its text and
    /// the IME's caret in it (byte offsets). Empty: none any more.
    Preedit { text: String, cursor: Option<(usize, usize)> },
    /// Text a composition turned into (`InputEvent::ImeCommit`).
    Commit(String),
    /// One character deleted.
    Backspace,
    /// The Return key (software or hardware keyboard), from the field's
    /// delegate (`textFieldShouldReturn:`).
    Enter,
    /// A hardware key that types nothing the field could pass on (Esc, an
    /// arrow, Tab, Ctrl+C …), pressed or released — what a desktop keyboard
    /// sends for it. `chars` is the text the desktop runtime hands with the
    /// press (Ctrl+C: `c`), so the app sees the same pair it does there.
    Key { key: Key, modifiers: Modifiers, chars: Vec<char>, pressed: bool },
}

/// A claimed hardware key that is down: what its release sends, and what
/// repeats while it is held.
struct Held {
    code: isize,
    key: Key,
    modifiers: Modifiers,
    chars: Vec<char>,
}

thread_local! {
    /// Deltas derived from `editingChanged`, drained per frame.
    static QUEUE: RefCell<VecDeque<KbEvent>> = const { RefCell::new(VecDeque::new()) };
    /// The attached shim (created once, on the first frame after the window exists).
    static SHIM: RefCell<Option<Shim>> = const { RefCell::new(None) };
    /// Last committed text seen (the field's text less the composition), so
    /// we can diff into insert/delete deltas.
    static LAST_TEXT: RefCell<String> = const { RefCell::new(String::new()) };
    /// Last composition sent as a preedit, and its caret.
    static LAST_MARKED: RefCell<(String, Option<(usize, usize)>)> = const { RefCell::new((String::new(), None)) };
    /// The hardware keys whose press we took: their release is ours too, not
    /// the field's (even if a composition started in between).
    static CLAIMED: RefCell<Vec<Held>> = const { RefCell::new(Vec::new()) };
    /// The key that repeats while held: the last one claimed and still down.
    static REPEAT: RefCell<Option<Held>> = const { RefCell::new(None) };
    /// The app's caret `(x, y, height)` in points, kept for a field not made yet.
    static CARET: Cell<Option<(f32, f32, f32)>> = const { Cell::new(None) };
    /// The hardware Return last pressed came while a composition was open:
    /// it picks the candidate, and the delegate's Return that iOS still sends
    /// for it is not a line (`should_return` takes it).
    static RETURN_CONFIRMS: Cell<bool> = const { Cell::new(false) };
}

/// How long a claimed key is held before it repeats, then how often. UIKit's
/// own repeat (for the keys the field types) isn't reachable from here; these
/// are close to its feel.
const REPEAT_DELAY: f64 = 0.4;
const REPEAT_EVERY: f64 = 0.05;

struct Shim {
    field: Retained<KbField>,
    /// Kept alive: the field holds only a weak reference to its action target
    /// (and delegate); it also runs the repeat timer and the input-mode watch.
    target: Retained<KbTarget>,
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

fn push(ev: KbEvent) {
    QUEUE.with(|q| q.borrow_mut().push_back(ev));
}

/// Diff `new` against the last committed text and enqueue the change as
/// backspaces (for the removed tail) followed by an insert (for the added
/// tail) — a commit when a composition was open (`composed`): the converted
/// text appears as the composition goes.
fn push_delta(new: &str, composed: bool) {
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
                q.push_back(if composed { KbEvent::Commit(added) } else { KbEvent::Text(added) });
            }
        });
        *lt.borrow_mut() = new.to_string();
    });
}

/// The field's text split at its composition: the committed text (around
/// it), the composition, and the IME's caret in the composition as byte
/// offsets. UITextField's positions count UTF-16 units.
fn split_marked(field: &UITextField) -> (String, String, Option<(usize, usize)>) {
    let text: Vec<u16> = field.text().map(|s| s.to_string()).unwrap_or_default().encode_utf16().collect();
    let Some(range) = field.markedTextRange() else {
        return (String::from_utf16_lossy(&text), String::new(), None);
    };
    let begin = field.beginningOfDocument();
    let at = |p: &UITextPosition| (field.offsetFromPosition_toPosition(&begin, p).max(0) as usize).min(text.len());
    let (a, b) = (at(&range.start()), at(&range.end()));
    let (s, e) = (a.min(b), a.max(b));
    let committed: Vec<u16> = text[..s].iter().chain(&text[e..]).copied().collect();
    let byte = |u: usize| String::from_utf16_lossy(&text[s..u.clamp(s, e)]).len();
    let caret = field.selectedTextRange().map(|r| (byte(at(&r.start())), byte(at(&r.end()))));
    (String::from_utf16_lossy(&committed), String::from_utf16_lossy(&text[s..e]), caret)
}

/// After an Enter, start the field over (unless a composition is open): the
/// line went out. Left in, the field grows with every line typed, and an IME
/// that takes back text before its composition — on Android it re-committed
/// what had been sent — would have the diff send old lines again.
fn forget_line(field: &UITextField) {
    if field.markedTextRange().is_none() {
        field.setText(None);
        LAST_TEXT.with(|lt| lt.borrow_mut().clear());
    }
}

/// Enqueue what the edit just made of the field: the committed delta, then
/// the composition if it changed (empty when it ended, so the app drops it).
fn on_edit(field: &UITextField) {
    let (committed, marked, caret) = split_marked(field);
    let composed = LAST_MARKED.with(|m| !m.borrow().0.is_empty());
    push_delta(&committed, composed);
    let next = (marked, caret);
    let changed = LAST_MARKED.with(|m| *m.borrow() != next);
    if changed {
        push(KbEvent::Preedit { text: next.0.clone(), cursor: next.1 });
        LAST_MARKED.with(|m| *m.borrow_mut() = next);
    }
}

// USB HID usages (`UIKeyboardHIDUsage`) of the keys handled by code.
const HID_RETURN: isize = 0x28;
const HID_ESCAPE: isize = 0x29;
const HID_BACKSPACE: isize = 0x2A;
const HID_TAB: isize = 0x2B;
const HID_SPACE: isize = 0x2C;
const HID_KEYPAD_ENTER: isize = 0x58;

/// The hardware key `k` as the app should get it, or `None` to leave it to
/// the field (it types something, or it's the system's).
///
/// Taken: the keys that type nothing — Esc, Tab, the arrows, Home/End/Page,
/// forward Delete, F1–F12 — Return with a modifier (Shift+Return is a new
/// line in a terminal's line editor, where plain Return goes through the
/// field's delegate), and Ctrl with any key. Left alone: everything while a
/// composition is open (Esc and the arrows are the IME's then), ⌘ chords
/// (the system's and the field's — ⌘V pastes into it), and Ctrl+Space, which
/// switches the input source on iPad.
fn claim(k: &UIKey, composing: bool) -> Option<(Key, Modifiers, Vec<char>)> {
    if composing {
        return None;
    }
    let flags = k.modifierFlags();
    let m = Modifiers {
        shift: flags.contains(UIKeyModifierFlags::Shift),
        ctrl: flags.contains(UIKeyModifierFlags::Control),
        alt: flags.contains(UIKeyModifierFlags::Alternate),
        meta: flags.contains(UIKeyModifierFlags::Command),
    };
    if m.meta {
        return None;
    }
    let code = k.keyCode().0;
    let named = match code {
        HID_ESCAPE => Some(Key::Escape),
        HID_TAB => Some(Key::Tab),
        HID_RETURN | HID_KEYPAD_ENTER if m.shift || m.alt || m.ctrl => Some(Key::Enter),
        0x3A..=0x45 => Some([
            Key::F1, Key::F2, Key::F3, Key::F4, Key::F5, Key::F6,
            Key::F7, Key::F8, Key::F9, Key::F10, Key::F11, Key::F12,
        ][(code - 0x3A) as usize]),
        0x49 => Some(Key::Insert),
        0x4A => Some(Key::Home),
        0x4B => Some(Key::PageUp),
        0x4C => Some(Key::Delete),
        0x4D => Some(Key::End),
        0x4E => Some(Key::PageDown),
        0x4F => Some(Key::Right),
        0x50 => Some(Key::Left),
        0x51 => Some(Key::Down),
        0x52 => Some(Key::Up),
        _ => None,
    };
    if let Some(key) = named {
        return Some((key, m, Vec::new()));
    }
    // Ctrl + a key; the modifier keys themselves (0xE0–0xE7) are not one.
    if !m.ctrl || code == HID_SPACE || (0xE0..=0xE7).contains(&code) {
        return None;
    }
    if code == HID_BACKSPACE {
        return Some((Key::Backspace, m, Vec::new()));
    }
    // By the character the key types in this layout, as winit's logical key
    // (and so the desktop runtime) names it.
    let plain = k.charactersIgnoringModifiers().to_string();
    let chars: Vec<char> = plain.chars().filter(|c| !c.is_control()).collect();
    if chars.is_empty() {
        return None;
    }
    let key = sabitori_window::keymap::key_from_winit(&winit::keyboard::Key::Character(plain.as_str().into()))?;
    Some((key, m, chars))
}

/// Run `f` on the target, if the shim is up.
fn with_target(f: impl FnOnce(&KbTarget)) {
    SHIM.with(|slot| {
        if let Some(shim) = slot.borrow().as_ref() {
            f(&shim.target);
        }
    });
}

/// Repeat `held` after [`REPEAT_DELAY`] (replacing any key repeating now).
fn start_repeat(held: Held) {
    stop_repeat();
    REPEAT.with(|r| *r.borrow_mut() = Some(held));
    with_target(|t| unsafe {
        let _: () = msg_send![t, performSelector: sel!(repeatKey:), withObject: None::<&AnyObject>, afterDelay: REPEAT_DELAY];
    });
}

fn stop_repeat() {
    REPEAT.with(|r| r.borrow_mut().take());
    with_target(|t| unsafe {
        let _: () = msg_send![NSObject::class(), cancelPreviousPerformRequestsWithTarget: t];
    });
}

/// Hand the claimed keys among `presses` to the app; what's left is the
/// field's. `None`: none left. `Some(None)`: all of them (pass `presses` on
/// as is). `Some(Some(set))`: those in `set`.
fn take_presses(field: &KbField, presses: &NSSet<UIPress>, began: bool) -> Option<Option<Retained<NSSet<UIPress>>>> {
    let mtm = MainThreadMarker::from(field);
    let composing = began && field.markedTextRange().is_some();
    let mut theirs: Vec<Retained<UIPress>> = Vec::new();
    let mut took = false;
    for press in presses.iter() {
        let mine = press.key(mtm).is_some_and(|k| {
            let code = k.keyCode().0;
            if began {
                if code == HID_RETURN || code == HID_KEYPAD_ENTER {
                    RETURN_CONFIRMS.set(composing);
                }
                let Some((key, modifiers, chars)) = claim(&k, composing) else {
                    return false;
                };
                push(KbEvent::Key { key, modifiers, chars: chars.clone(), pressed: true });
                if key == Key::Enter {
                    forget_line(field);
                }
                CLAIMED.with(|c| c.borrow_mut().push(Held { code, key, modifiers, chars: chars.clone() }));
                start_repeat(Held { code, key, modifiers, chars });
                true
            } else {
                let Some(held) = CLAIMED.with(|c| {
                    let mut c = c.borrow_mut();
                    let i = c.iter().position(|h| h.code == code)?;
                    Some(c.swap_remove(i))
                }) else {
                    return false;
                };
                if REPEAT.with(|r| r.borrow().as_ref().is_some_and(|h| h.code == code)) {
                    stop_repeat();
                }
                push(KbEvent::Key { key: held.key, modifiers: held.modifiers, chars: Vec::new(), pressed: false });
                true
            }
        });
        if mine {
            took = true;
        } else {
            theirs.push(press);
        }
    }
    if took {
        request_redraw();
    }
    match (took, theirs.is_empty()) {
        (_, true) => None,
        (false, false) => Some(None),
        (true, false) => Some(Some(NSSet::from_retained_slice(&theirs))),
    }
}

/// Predictions only while the keyboard is Japanese. iOS ties the prediction
/// bar to autocorrection, and autocorrection rewrites English as it's typed
/// (`ls` → `Is`, already sent to a terminal) — so it stays off for every
/// other input mode, and with it the bar. Japanese needs its predictions and
/// has no such rewriting. A trait change reaches a keyboard that is up only
/// through `reloadInputViews`.
fn fit_to_input_mode(field: &UITextField) {
    let japanese = field
        .textInputMode()
        .and_then(|m| m.primaryLanguage())
        .is_some_and(|l| l.to_string().starts_with("ja"));
    let want = if japanese { UITextAutocorrectionType::Default } else { UITextAutocorrectionType::No };
    if field.autocorrectionType() != want {
        field.setAutocorrectionType(want);
        if field.isFirstResponder() {
            field.reloadInputViews();
        }
    }
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
            on_edit(sender);
            request_redraw();
        }

        // The field's delegate: Return. Answering NO keeps the keyboard up —
        // a terminal takes line after line — and inserts nothing.
        //
        // A Return that ends a composition only confirms it, as on a desktop:
        // iOS asks here for it too (a hardware Return on a picked candidate),
        // and passed on as Enter it sent the line the moment the kanji was
        // chosen. Told apart by a composition still open, one the hardware
        // Return was pressed on, or a commit the app hasn't drained yet (the
        // IME ended it within this same keystroke).
        #[unsafe(method(textFieldShouldReturn:))]
        fn should_return(&self, field: &UITextField) -> bool {
            if field.markedTextRange().is_some() {
                field.unmarkText();
            }
            // What the field holds that went unseen goes out before the Enter.
            on_edit(field);
            let confirms = RETURN_CONFIRMS.take() || QUEUE.with(|q| q.borrow().iter().any(|e| matches!(e, KbEvent::Commit(_))));
            if !confirms {
                push(KbEvent::Enter);
                forget_line(field);
            }
            request_redraw();
            false
        }

        // `UITextInputCurrentInputModeDidChangeNotification`: the keyboard
        // switched language.
        #[unsafe(method(inputModeChanged:))]
        fn input_mode_changed(&self, _note: &NSNotification) {
            SHIM.with(|slot| {
                if let Some(shim) = slot.borrow().as_ref() {
                    fit_to_input_mode(&shim.field);
                }
            });
        }

        // The repeat of a held claimed key (`start_repeat`). Stops when the
        // key is gone or the field lost the keyboard (its release may never
        // come then).
        #[unsafe(method(repeatKey:))]
        fn repeat_key(&self, _arg: Option<&AnyObject>) {
            let up = SHIM.with(|slot| slot.borrow().as_ref().is_some_and(|s| s.field.isFirstResponder()));
            let again = REPEAT.with(|r| {
                r.borrow().as_ref().map(|h| KbEvent::Key {
                    key: h.key,
                    modifiers: h.modifiers,
                    chars: h.chars.clone(),
                    pressed: true,
                })
            });
            match again {
                Some(ev) if up => {
                    push(ev);
                    request_redraw();
                    unsafe {
                        let _: () = msg_send![self, performSelector: sel!(repeatKey:), withObject: None::<&AnyObject>, afterDelay: REPEAT_EVERY];
                    }
                }
                _ => {
                    REPEAT.with(|r| r.borrow_mut().take());
                }
            }
        }
    }
);

impl KbTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

impl KbField {
    fn after_input(&self) {
        on_edit(self);
        request_redraw();
    }
}

// The hidden field: a `UITextField` that also hands on the keys it would drop
// or misuse (see the module doc).
define_class!(
    #[unsafe(super(UITextField, UIControl, UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SabitoriKbField"]
    struct KbField;

    unsafe impl NSObjectProtocol for KbField {}

    impl KbField {
        #[unsafe(method(pressesBegan:withEvent:))]
        fn presses_began(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            match take_presses(self, presses, true) {
                None => {}
                Some(None) => unsafe { msg_send![super(self), pressesBegan: presses, withEvent: event] },
                Some(Some(rest)) => unsafe { msg_send![super(self), pressesBegan: &*rest, withEvent: event] },
            }
        }

        #[unsafe(method(pressesEnded:withEvent:))]
        fn presses_ended(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            match take_presses(self, presses, false) {
                None => {}
                Some(None) => unsafe { msg_send![super(self), pressesEnded: presses, withEvent: event] },
                Some(Some(rest)) => unsafe { msg_send![super(self), pressesEnded: &*rest, withEvent: event] },
            }
        }

        #[unsafe(method(pressesCancelled:withEvent:))]
        fn presses_cancelled(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            match take_presses(self, presses, false) {
                None => {}
                Some(None) => unsafe { msg_send![super(self), pressesCancelled: presses, withEvent: event] },
                Some(Some(rest)) => unsafe { msg_send![super(self), pressesCancelled: &*rest, withEvent: event] },
            }
        }

        // Backspace with nothing in the field deletes nothing there, so no
        // `editingChanged` says it was pressed — but the terminal (or the
        // app's own field) has text before the cursor. Both keyboards call
        // this, the software one and a hardware one.
        // The text-input calls that change the text or the composition.
        // `editingChanged` doesn't come for every one of them (ending a
        // composition changes no character), so each looks at the field
        // after it; `on_edit` sends only what changed, so a second look
        // sends nothing.
        #[unsafe(method(insertText:))]
        fn insert_text(&self, text: &NSString) {
            unsafe { msg_send![super(self), insertText: text] }
            self.after_input();
        }

        #[unsafe(method(setMarkedText:selectedRange:))]
        fn set_marked_text(&self, text: Option<&NSString>, range: NSRange) {
            unsafe { msg_send![super(self), setMarkedText: text, selectedRange: range] }
            self.after_input();
        }

        #[unsafe(method(unmarkText))]
        fn unmark_text(&self) {
            unsafe { msg_send![super(self), unmarkText] }
            self.after_input();
        }

        #[unsafe(method(deleteBackward))]
        fn delete_backward(&self) {
            if self.text().is_none_or(|t| t.length() == 0) {
                push(KbEvent::Backspace);
                request_redraw();
            }
            unsafe { msg_send![super(self), deleteBackward] }
        }
    }
);

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
        let field: Retained<KbField> = unsafe { msg_send![KbField::alloc(mtm), init] };
        let target = KbTarget::new(mtm);
        let target_obj: &AnyObject = &target;
        unsafe {
            field.addTarget_action_forControlEvents(
                Some(target_obj),
                sel!(textChanged:),
                UIControlEvents::EditingChanged,
            );
            // Weak on UIKit's side; `Shim::target` keeps it alive. UIKit asks
            // the delegate with `respondsToSelector:`, so no protocol needed.
            let _: () = msg_send![&*field, setDelegate: target_obj];
            // What's typed goes out key by key (into a terminal, a code field):
            // no capital first letter, no autocorrect, no smart quotes/dashes.
            // UITextInputTraits values: autocapitalization None = 0; the
            // others' No = 1. Autocorrection follows the input mode from here
            // on (`fit_to_input_mode`).
            let _: () = msg_send![&*field, setAutocapitalizationType: 0isize];
            let _: () = msg_send![&*field, setAutocorrectionType: 1isize];
            let _: () = msg_send![&*field, setSpellCheckingType: 1isize];
            let _: () = msg_send![&*field, setSmartQuotesType: 1isize];
            let _: () = msg_send![&*field, setSmartDashesType: 1isize];
            let _: () = msg_send![&*field, setSmartInsertDeleteType: 1isize];
            // No buttons in the bar iPadOS shows over a hardware keyboard
            // (undo / redo / paste): a terminal has its own, and with nothing
            // to show the bar gets out of the way.
            let item: Retained<AnyObject> = msg_send![&*field, inputAssistantItem];
            let none = NSArray::<AnyObject>::new();
            let _: () = msg_send![&*item, setLeadingBarButtonGroups: &*none];
            let _: () = msg_send![&*item, setTrailingBarButtonGroups: &*none];
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                target_obj,
                sel!(inputModeChanged:),
                Some(UITextInputCurrentInputModeDidChangeNotification),
                None,
            );
            // Zero wide at the caret (`set_caret_area`): its text and caret
            // must not spill out of it.
            field.setClipsToBounds(true);
            if let Some(area) = CARET.with(Cell::get) {
                place(&field, area);
            }
            let host: &UIView = &*(ui_view_ptr as *const UIView);
            host.addSubview(&field);
        }
        *slot.borrow_mut() = Some(Shim {
            field,
            target,
            window: Arc::clone(window),
        });
    });
}

/// Put the field at the app's caret `(x, y)`, `height` tall and zero wide —
/// clipped, so nothing of it shows. The system draws what belongs to the
/// field's caret there: a hardware keyboard's conversion candidates, and the
/// `a` / `あ` that tells the input source switched. At the view's origin both
/// sat in the top-left corner, cut off by the screen's edge.
pub fn set_caret_area(x: f32, y: f32, height: f32) {
    CARET.with(|c| c.set(Some((x, y, height))));
    SHIM.with(|slot| {
        if let Some(shim) = slot.borrow().as_ref() {
            place(&shim.field, (x, y, height));
        }
    });
}

fn place(field: &UITextField, (x, y, h): (f32, f32, f32)) {
    field.setFrame(CGRect::new(CGPoint::new(x.into(), y.into()), CGSize::new(0.0, h.max(1.0).into())));
}

/// Raise (`true`) or dismiss (`false`) the software keyboard. The runtime
/// raises it only for a tap on something that takes typing — like Android, a
/// focused terminal at launch must not pop it up — and dismisses it when focus
/// goes or the app disallows it. Whether the field is first responder decides
/// what to do, not a remembered flag: the keyboard's own dismiss key resigns
/// the field behind our back, and the next tap must raise it again.
pub fn set_active(active: bool) {
    let field = SHIM.with(|slot| slot.borrow().as_ref().map(|s| s.field.clone()));
    let Some(field) = field else {
        return;
    };
    if field.isFirstResponder() == active {
        return;
    }
    if active {
        let _: bool = field.becomeFirstResponder();
        fit_to_input_mode(&field);
    } else {
        let _: bool = field.resignFirstResponder();
        stop_repeat();
        CLAIMED.with(|c| c.borrow_mut().clear());
        RETURN_CONFIRMS.set(false);
        // Reset the field + diff baseline so the next focus starts clean; a
        // composition left open is gone with it.
        field.setText(None);
        LAST_TEXT.with(|lt| lt.borrow_mut().clear());
        if LAST_MARKED.with(|m| !std::mem::take(&mut *m.borrow_mut()).0.is_empty()) {
            push(KbEvent::Preedit { text: String::new(), cursor: None });
        }
    }
}

/// Drain queued text deltas (FIFO). Collected into a `Vec` first so the
/// runtime's routing can't re-enter the borrow.
pub fn drain() -> Vec<KbEvent> {
    QUEUE.with(|q| q.borrow_mut().drain(..).collect())
}

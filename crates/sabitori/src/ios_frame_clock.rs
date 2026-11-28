//! iOS: frames on the display's clock (`CADisplayLink`).
//!
//! iOS picks a ProMotion screen's rate from what apps ask for; a display link
//! asks (`preferredFrameRateRange`), and its tick, one per refresh, is when a
//! frame starts — the frames come evenly. Started by the runtime's own timer
//! instead, mearie's frames on a 120 Hz iPad stayed on screen 12.5 and 20.8 ms
//! by turns (Instruments, 2026-11-26).
//!
//! This alone didn't lift the 60 fps it scrolled at: that was the drawables
//! (two, so every other refresh — see `desired_maximum_frame_latency` in
//! sabitori-gpu's renderer). With three as well, 108–115 fps.
//!
//! It runs only while the runtime has something to draw (not parked). Left
//! running, it would wake the app every refresh — the battery drain the
//! parking was written to stop ([`crate::wake`]).
//!
//! Main thread only, as UIKit and winit-iOS are.

#![cfg(target_os = "ios")]

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_foundation::{NSObject, NSObjectProtocol, NSRunLoop, NSRunLoopCommonModes};
use objc2_quartz_core::{CADisplayLink, CAFrameRateRange};

thread_local! {
    /// The link and the object it ticks (it keeps that alive itself; kept
    /// here too so the pair goes together). Made on first use.
    static LINK: RefCell<Option<(Retained<CADisplayLink>, Retained<Tick>)>> = const { RefCell::new(None) };
    /// A refresh came and no frame has started since.
    static DUE: Cell<bool> = const { Cell::new(false) };
    /// The rate asked for (frames per second), or 0 while stopped.
    static RATE: Cell<f32> = const { Cell::new(0.0) };
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SabitoriFrameClock"]
    struct Tick;

    unsafe impl NSObjectProtocol for Tick {}

    impl Tick {
        // Each refresh. Waking the run loop is enough: its next pass runs the
        // runtime's `about_to_wait`, which sees the tick and starts a frame.
        #[unsafe(method(tick:))]
        fn tick(&self, _link: &CADisplayLink) {
            DUE.set(true);
        }
    }
);

/// The rate for a frame interval the app asked for: the screen's top rate
/// (120 Hz) for one shorter than a 90 Hz frame (a scroll, at 8 ms), else 60.
pub(crate) fn rate_for(interval: std::time::Duration) -> f32 {
    if interval.as_secs_f32() < 1.0 / 90.0 { 120.0 } else { 60.0 }
}

/// Tick at `rate` frames per second, or stop (`None`).
pub(crate) fn run(rate: Option<f32>) {
    let want = rate.unwrap_or(0.0);
    if RATE.get() == want {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    LINK.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            if rate.is_none() {
                return;
            }
            let tick: Retained<Tick> = unsafe { msg_send![Tick::alloc(mtm), init] };
            let target: &AnyObject = &tick;
            let link = unsafe { CADisplayLink::displayLinkWithTarget_selector(target, sel!(tick:)) };
            unsafe { link.addToRunLoop_forMode(&NSRunLoop::mainRunLoop(), NSRunLoopCommonModes) };
            *slot = Some((link, tick));
        }
        let Some((link, _)) = slot.as_ref() else { return };
        match rate {
            Some(r) => {
                // As Apple suggests for a ProMotion screen: down to two thirds
                // of it when the system must (thermal, Low Power Mode).
                link.setPreferredFrameRateRange(CAFrameRateRange { minimum: r * 2.0 / 3.0, maximum: r, preferred: r });
                link.setPaused(false);
            }
            None => {
                link.setPaused(true);
                DUE.set(false);
            }
        }
        RATE.set(want);
    });
}

/// The link is ticking: frames start on its ticks, not on a timer.
pub(crate) fn running() -> bool {
    RATE.get() > 0.0
}

/// A refresh came since the last frame started. Taking it starts one.
pub(crate) fn take_due() -> bool {
    DUE.replace(false)
}

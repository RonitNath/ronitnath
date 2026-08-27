//! The socket itself: where it points, how long it may stay silent, and how a
//! task waits without a runtime.
//!
//! A browser has no sleep, so the delays a reconnect loop needs are built from
//! `setTimeout` and a waker. Everything here is browser-only in effect: it
//! compiles natively so the crate's tests can, and panics if anything ever
//! calls it there, which nothing does.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use leptos::prelude::set_timeout;

/// The subscription endpoint on this origin, as a WebSocket URL.
///
/// Derived from the page's own location rather than configured: a bundle talks
/// to the server that served it, over the same scheme's socket.
pub fn sub_url() -> String {
    let location = leptos::prelude::window().location();
    let host = location.host().unwrap_or_default();
    let scheme = match location.protocol().unwrap_or_default().as_str() {
        "https:" => "wss",
        _ => "ws",
    };
    format!("{scheme}://{host}/api/sub")
}

/// A `[0, 1)` sample for the reconnect jitter.
pub fn random() -> f64 {
    js_sys::Math::random()
}

/// Wait, by way of one `setTimeout`.
pub async fn sleep(duration: Duration) {
    let state = Rc::new(RefCell::new(Timer {
        fired: false,
        waker: None,
    }));
    let armed = Rc::clone(&state);
    set_timeout(
        move || {
            let mut timer = armed.borrow_mut();
            timer.fired = true;
            if let Some(waker) = timer.waker.take() {
                waker.wake();
            }
        },
        duration,
    );
    Sleep { state }.await
}

struct Timer {
    fired: bool,
    waker: Option<Waker>,
}

struct Sleep {
    state: Rc<RefCell<Timer>>,
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut timer = self.state.borrow_mut();
        if timer.fired {
            Poll::Ready(())
        } else {
            timer.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

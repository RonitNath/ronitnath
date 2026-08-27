//! Driving the atlas: drag, scroll, pinch, arrows, and the bar's own controls.
//!
//! Two kinds of listener, on purpose. The ones on the dialog die with it — the
//! node is removed and the browser collects them — while the two that must
//! outlive any single opening, the document's keys and the window's resize,
//! are wired once for the life of the page and look up whichever atlas is
//! currently open. Wiring those per-opening is how a dialog that can be opened
//! and closed all afternoon accumulates a hundred live handlers.

use std::cell::Cell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::{Event, KeyboardEvent, PointerEvent, WheelEvent};

use crate::dom;
use crate::explorer::{ACTIVE, Atlas};

/// One notch of the zoom controls, and how far an arrow key pans.
const ZOOM_STEP: f64 = 1.5;
const KEY_PAN_PX: f64 = 30.0;

thread_local! {
    static GLOBALS_WIRED: Cell<bool> = const { Cell::new(false) };
}

pub fn listen(atlas: &Rc<Atlas>) {
    wire_globals();
    wire_dialog(atlas);
}

fn with_active(act: impl FnOnce(&Rc<Atlas>)) {
    let atlas = ACTIVE.with(|active| active.borrow().clone());
    if let Some(atlas) = atlas {
        act(&atlas);
    }
}

/// The two listeners that outlive one opening of the atlas.
fn wire_globals() {
    if GLOBALS_WIRED.with(|wired| wired.replace(true)) {
        return;
    }
    if let Some(document) = dom::document() {
        dom::listen::<KeyboardEvent>(document.as_ref(), "keydown", |event| {
            with_active(|atlas| key(atlas, &event));
        });
    }
    if let Some(window) = dom::window() {
        dom::listen::<Event>(window.as_ref(), "resize", |_| {
            with_active(|atlas| {
                atlas.resize();
                atlas.draw(dom::now_ms());
            });
        });
    }
}

fn key(atlas: &Rc<Atlas>, event: &KeyboardEvent) {
    match event.key().as_str() {
        "Escape" => super::close(),
        " " | "Spacebar" => {
            event.prevent_default();
            atlas.toggle_pause();
        }
        "ArrowLeft" => pan(atlas, -KEY_PAN_PX, 0.0, event),
        "ArrowRight" => pan(atlas, KEY_PAN_PX, 0.0, event),
        "ArrowUp" => pan(atlas, 0.0, -KEY_PAN_PX, event),
        "ArrowDown" => pan(atlas, 0.0, KEY_PAN_PX, event),
        _ => {}
    }
}

fn pan(atlas: &Rc<Atlas>, dx: f64, dy: f64, event: &KeyboardEvent) {
    event.prevent_default();
    let (width, height) = atlas.surface.borrow().css_size();
    atlas.release_aim();
    atlas.steer(|camera| camera.pan(dx, dy, width, height));
}

fn wire_dialog(atlas: &Rc<Atlas>) {
    let node = atlas.node.clone();
    // Pointer gestures belong to the canvas, not to the dialog. Capturing a
    // pointer that went down on a button retargets its `pointerup` to the
    // capturing element, and the browser then fires the resulting `click` on
    // that element instead of the button — every control in the bar goes dead.
    let Ok(Some(canvas)) = node.query_selector(".atlas-canvas") else {
        return;
    };

    dom::listen::<Event>(node.as_ref(), "click", |event| {
        let action = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
            .and_then(|element| element.closest("[data-action]").ok().flatten())
            .and_then(|element| element.get_attribute("data-action"));
        let Some(action) = action else { return };
        with_active(|atlas| match action.as_str() {
            "close" => super::close(),
            "pause" => atlas.toggle_pause(),
            "zoom-in" => atlas.steer(|camera| {
                camera.zoom(1.0 / ZOOM_STEP);
            }),
            "zoom-out" => atlas.steer(|camera| {
                camera.zoom(ZOOM_STEP);
            }),
            _ => {}
        });
    });

    dom::listen::<WheelEvent>(canvas.as_ref(), "wheel", |event| {
        event.prevent_default();
        with_active(|atlas| {
            // Exponential in the scroll delta, so a trackpad flick and a mouse
            // notch both feel like the same amount of zoom per unit of gesture.
            let factor = (event.delta_y() * 0.001).exp();
            atlas.steer(|camera| {
                camera.zoom(factor);
            });
        });
    });

    let down = canvas.clone();
    dom::listen::<PointerEvent>(canvas.as_ref(), "pointerdown", move |event| {
        with_active(|atlas| {
            if let Some(element) = down.dyn_ref::<web_sys::HtmlElement>() {
                let _ = element.set_pointer_capture(event.pointer_id());
            }
            let mut pointers = atlas.pointers.borrow_mut();
            pointers.retain(|(id, _, _)| *id != event.pointer_id());
            pointers.push((
                event.pointer_id(),
                f64::from(event.client_x()),
                f64::from(event.client_y()),
            ));
            if let Some(distance) = spread(&pointers) {
                atlas.pinch.set(Some((distance, atlas.camera().fov)));
            }
        });
    });

    dom::listen::<PointerEvent>(canvas.as_ref(), "pointermove", |event| {
        with_active(|atlas| {
            let moved = {
                let mut pointers = atlas.pointers.borrow_mut();
                let Some(slot) = pointers
                    .iter_mut()
                    .find(|(id, _, _)| *id == event.pointer_id())
                else {
                    return;
                };
                let delta = (
                    f64::from(event.client_x()) - slot.1,
                    f64::from(event.client_y()) - slot.2,
                );
                slot.1 = f64::from(event.client_x());
                slot.2 = f64::from(event.client_y());
                (delta, spread(&pointers))
            };
            let ((dx, dy), distance) = moved;
            match (distance, atlas.pinch.get()) {
                (Some(distance), Some((from, fov))) if distance > 0.0 => {
                    // Fingers apart is a closer look: the field narrows by the
                    // ratio the gesture opened by.
                    atlas.steer(|camera| {
                        camera.fov = fov;
                        camera.zoom(from / distance);
                    });
                }
                _ => {
                    let (width, height) = atlas.surface.borrow().css_size();
                    atlas.release_aim();
                    atlas.steer(|camera| camera.pan(dx, dy, width, height));
                }
            }
        });
    });

    for ending in ["pointerup", "pointercancel", "pointerleave"] {
        dom::listen::<PointerEvent>(canvas.as_ref(), ending, |event| {
            with_active(|atlas| {
                atlas
                    .pointers
                    .borrow_mut()
                    .retain(|(id, _, _)| *id != event.pointer_id());
                if atlas.pointers.borrow().len() < 2 {
                    atlas.pinch.set(None);
                }
            });
        });
    }
}

/// The distance between exactly two pointers — a pinch and nothing else.
fn spread(pointers: &[(i32, f64, f64)]) -> Option<f64> {
    match pointers {
        [(_, ax, ay), (_, bx, by)] => Some((ax - bx).hypot(ay - by)),
        _ => None,
    }
}

//! Light and dark.
//!
//! Both palettes are authored in `tokens.css`; the only thing switched here is
//! which one the document resolves against, by way of `color-scheme`. The
//! choice is stored per browser, and until someone makes one the page follows
//! the operating system.

use leptos::prelude::*;

/// Where the choice is kept.
const KEY: &str = "rn-theme";

/// The two modes. There is no third, and no "system" value stored — an absent
/// key *is* system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The light palette.
    Light,
    /// The dark palette.
    Dark,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }

    fn other(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }
}

/// The mode in force: the stored choice, else what the system asks for.
pub fn current() -> Mode {
    stored()
        .or_else(|| {
            window()
                .match_media("(prefers-color-scheme: dark)")
                .ok()
                .flatten()
                .map(|query| {
                    if query.matches() {
                        Mode::Dark
                    } else {
                        Mode::Light
                    }
                })
        })
        .unwrap_or(Mode::Dark)
}

/// Put `mode` on the document and remember it.
pub fn set(mode: Mode) {
    if let Some(root) = document().document_element() {
        let _ = root.set_attribute("data-theme", mode.as_str());
    }
    if let Ok(Some(storage)) = window().local_storage() {
        let _ = storage.set_item(KEY, mode.as_str());
    }
}

fn stored() -> Option<Mode> {
    window()
        .local_storage()
        .ok()
        .flatten()
        .and_then(|storage| storage.get_item(KEY).ok().flatten())
        .as_deref()
        .and_then(Mode::parse)
}

/// The footer's theme control. It is labelled with the mode it switches to,
/// because a control says what it does.
#[component]
pub fn ThemeToggle() -> impl IntoView {
    let mode = RwSignal::new(current());
    Effect::new(move |_| set(mode.get()));
    view! {
        <button type="button" on:click=move |_| mode.update(|m| *m = m.other())>
            {move || match mode.get().other() {
                Mode::Light => "Light",
                Mode::Dark => "Dark",
            }}
        </button>
    }
}

//! Which organization this bundle is looking at.
//!
//! The `/org` tier is held by an operator of *at least one* organization, so
//! every page needs to know which. That choice lives here: it comes from
//! `whoami` — the only list of organizations the chrome is given — is narrowed
//! to the ones the reader actually operates, and is remembered per browser so
//! a reload does not send an operator of six back to the first one.
//!
//! The switcher renders only when there is more than one to switch between; a
//! control with a single option is a label pretending to be a control.

use leptos::prelude::*;
use rn_api::{MemberRole, OrganizationRef};
use rn_ui::use_whoami;

/// Where the choice is kept, per browser.
const KEY: &str = "rn-org";

/// The organization in force, as a context every page reads.
#[derive(Clone, Copy)]
pub struct Scope {
    chosen: RwSignal<Option<String>>,
}

impl Scope {
    /// The organizations this reader operates, in the order `whoami` gave them.
    pub fn operated(self) -> Vec<OrganizationRef> {
        use_whoami()
            .get()
            .map(|whoami| {
                whoami
                    .organizations
                    .into_iter()
                    .filter(|org| matches!(org.role, MemberRole::Admin | MemberRole::Owner))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The organization in force: the remembered one if it is still operated,
    /// otherwise the first. `None` until `whoami` has arrived.
    pub fn current(self) -> Option<OrganizationRef> {
        let operated = self.operated();
        let chosen = self.chosen.get();
        operated
            .iter()
            .find(|org| Some(org.public_id.as_str().to_owned()) == chosen)
            .or_else(|| operated.first())
            .cloned()
    }

    /// The organization's public id, which is what every query is scoped by.
    pub fn id(self) -> Option<String> {
        self.current().map(|org| org.public_id.as_str().to_owned())
    }

    /// Choose one, and remember it.
    pub fn choose(self, id: String) {
        if let Ok(Some(storage)) = window().local_storage() {
            let _ = storage.set_item(KEY, &id);
        }
        self.chosen.set(Some(id));
    }
}

/// Read the scope. Panics outside the provider, which is the root.
pub fn use_scope() -> Scope {
    use_context().expect("use_scope outside the /org bundle root")
}

/// Install the scope for the whole bundle.
pub fn provide_scope() -> Scope {
    let stored = window()
        .local_storage()
        .ok()
        .flatten()
        .and_then(|storage| storage.get_item(KEY).ok().flatten());
    let scope = Scope {
        chosen: RwSignal::new(stored),
    };
    provide_context(scope);
    scope
}

/// The switcher, above the page. It appears when there is a choice to make.
#[component]
pub fn Switcher() -> impl IntoView {
    let scope = use_scope();
    move || {
        let operated = scope.operated();
        if operated.len() < 2 {
            return None;
        }
        let current = scope.id();
        let options = operated
            .into_iter()
            .map(|org| {
                let id = org.public_id.as_str().to_owned();
                let selected = current.as_deref() == Some(id.as_str());
                view! {
                    <option value=id selected=selected>
                        {org.display}
                    </option>
                }
            })
            .collect_view();
        Some(view! {
            <div class="switcher">
                <select
                    aria-label="Organization"
                    on:change=move |event| scope.choose(event_target_value(&event))
                >
                    {options}
                </select>
            </div>
        })
    }
}

/// Render `view` once the scope is known, and the decline page when this
/// reader operates nothing at all.
#[component]
pub fn Scoped<F, V>(
    /// Built with the organization's public id.
    view: F,
) -> impl IntoView
where
    F: Fn(String) -> V + Send + Sync + 'static,
    V: IntoView + 'static,
{
    let scope = use_scope();
    let who = use_whoami();
    move || {
        // Nothing at all until `whoami` lands: an empty page for one frame is
        // better than a decline for one frame.
        who.get()?;
        match scope.id() {
            Some(id) => Some(view(id).into_any()),
            None => Some(view! { <rn_ui::Decline /> }.into_any()),
        }
    }
}

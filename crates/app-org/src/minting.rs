//! Minting an invitation, wherever one is minted.
//!
//! An organization is its own root group, so the control is the same on the
//! members page and inside a group: pick the role and how long it lasts, and
//! the claim URL comes back once — in the reply, because the row keeps only the
//! token's SHA-256 and nothing can reproduce it afterwards. Which is why it
//! arrives with the button that copies it.

use leptos::prelude::*;
use rn_api::MemberRole;
use rn_api::commands::Invite;

use crate::bits::{Choice, Copyable, Note, act};

/// A day, in seconds — the unit an invitation's life is set in.
const DAY: i64 = 60 * 60 * 24;

/// The control that mints a link into one container.
#[component]
pub fn Mint(
    /// The container to invite into: an organization or one of its groups.
    container: String,
) -> impl IntoView {
    let role = RwSignal::new("member".to_owned());
    let days = RwSignal::new("7".to_owned());
    let minted = RwSignal::new(None::<String>);
    let note = RwSignal::new(None::<String>);
    let container = StoredValue::new(container);

    view! {
        <div class="minting">
            <Choice label="Role" options=vec![("member", "member"), ("admin", "admin")] value=role />
            <Choice
                label="Lasts"
                options=vec![("1", "a day"), ("7", "a week"), ("30", "a month")]
                value=days
            />
            <button
                type="button"
                class="commit"
                on:click=move |_| {
                    let Ok(group) = container.get_value().parse() else {
                        return;
                    };
                    let role = if role.get_untracked() == "admin" {
                        MemberRole::Admin
                    } else {
                        MemberRole::Member
                    };
                    let lasts = days.get_untracked().parse::<i64>().unwrap_or(7) * DAY;
                    let expires_at = (js_sys::Date::now() / 1000.0) as i64 + lasts;
                    act(note, Invite { group, role, expires_at }, move |result| {
                        minted
                            .set(
                                result["token"]
                                    .as_str()
                                    .map(|token| format!("/links/{token}")),
                            );
                    });
                }
            >
                "Mint link"
            </button>
            <Note note=note />
        </div>
        {move || {
            minted
                .get()
                .map(|path| {
                    let url = format!(
                        "{}{path}",
                        window().location().origin().unwrap_or_default(),
                    );
                    view! {
                        <div class="claim">
                            <Copyable value=url label="Copy the claim link" />
                        </div>
                    }
                })
        }}
    }
}

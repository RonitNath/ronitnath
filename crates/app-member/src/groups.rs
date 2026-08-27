//! `/app/groups` — the groups this person belongs to, who else is in them, and
//! the invitations that put people there.
//!
//! A group is picked, not paged: the roster, the role controls and the
//! invitation all act on one group, and having them act on "whichever row you
//! last clicked" is how a role gets set on the wrong people. The picked group
//! is in the page's own state and nowhere else — it is not a route, because it
//! is not a place.

use leptos::prelude::*;
use rn_api::commands::{CreateGroup, Invite, Leave, SetRole};
use rn_api::whoami::MemberRole;
use rn_ui::{Commit, Live, PageHead};

use crate::api::{Refusal, attempt};
use crate::parts::{Act, Carry, Note, Pair, Section, titled, when};
use crate::rows::{Group, Member};

/// A day, which is what an invitation is worth unless somebody says otherwise.
const DAY: i64 = 60 * 60 * 24;

#[component]
pub fn Groups() -> impl IntoView {
    let groups = Live::<Group>::subscribe("groups", &[]);
    let members = Live::<Member>::subscribe("group-members", &[]);
    let refusal = RwSignal::new(None::<Refusal>);
    let picked = RwSignal::new(None::<String>);

    // The first group is the one shown until somebody picks another, so the
    // page is never a list with nothing under it.
    let current = Signal::derive(move || {
        picked
            .get()
            .or_else(|| groups.rows().first().map(|group| group.public_id.clone()))
    });
    let group_of = move || {
        let id = current.get()?;
        groups
            .rows()
            .into_iter()
            .find(|group| group.public_id == id)
    };

    let list = move || {
        let rows = groups.rows();
        if rows.is_empty() {
            return view! { <p class="quiet">"You are in no group."</p> }.into_any();
        }
        rows.into_iter()
            .map(|group| {
                let id = group.public_id.clone();
                let chosen = current.get().as_deref() == Some(group.public_id.as_str());
                let pick = Callback::new(move |()| picked.set(Some(id.clone())));
                view! {
                    <Pair label=titled(&group.role)>
                        <span class="actions">
                            <Act
                                label=group.display.clone()
                                on_act=pick
                                disabled=Signal::derive(move || chosen)
                            />
                        </span>
                    </Pair>
                }
            })
            .collect_view()
            .into_any()
    };

    let roster = move || {
        let Some(id) = current.get() else {
            return ().into_any();
        };
        let rows: Vec<Member> = members
            .rows()
            .into_iter()
            .filter(|member| member.group == id)
            .collect();
        if rows.is_empty() {
            return view! {
                <tr>
                    <td class="empty" colspan="4">"Nobody yet."</td>
                </tr>
            }
            .into_any();
        }
        rows.into_iter()
            .map(|member| view! { <Rostered member=member refusal=refusal /> })
            .collect_view()
            .into_any()
    };

    let leaving = move || {
        let group = group_of()?;
        let id = group.public_id.clone();
        let leave = Callback::new(move |()| {
            let Ok(group) = id.parse() else {
                return;
            };
            attempt(Leave { group }, refusal, move |_| picked.set(None));
        });
        Some(view! {
            <span class="actions">
                <Act label="Leave this group" undo=true on_act=leave />
            </span>
        })
    };

    view! {
        <PageHead title="Groups" />
        <div class="sections">
            <div class="column">
                <Section title="Your groups">
                    <dl class="kv">{list}</dl>
                </Section>
                <Making refusal=refusal />
            </div>
            <div class="column">
                <section class="section">
                <h2>
                    {move || group_of().map_or_else(|| "Members".to_owned(), |group| group.display)}
                </h2>
                <table class="tbl">
                    <thead>
                        <tr>
                            <th class="p1" scope="col">"Member"</th>
                            <th class="p1" scope="col">"Role"</th>
                            <th class="p2" scope="col">"Contact"</th>
                            <th class="p3" scope="col">"Joined"</th>
                        </tr>
                    </thead>
                    <tbody>{roster}</tbody>
                </table>
                    {leaving}
                </section>
                <Inviting group=current refusal=refusal />
            </div>
        </div>
        <Note refusal=refusal />
    }
}

/// One person in the roster. Their role is a field, so changing it syncs on
/// the change rather than waiting for a button nobody would press.
#[component]
fn Rostered(member: Member, refusal: RwSignal<Option<Refusal>>) -> impl IntoView {
    let group = member.group.clone();
    let party = member.public_id.clone();
    let held = member.role.clone();
    let set = move |event: leptos::ev::Event| {
        let wanted = event_target_value(&event);
        let (Some(party), Ok(group)) = (party.clone(), group.parse()) else {
            return;
        };
        let Ok(party) = party.parse() else {
            return;
        };
        let role = match wanted.as_str() {
            "owner" => MemberRole::Owner,
            "admin" => MemberRole::Admin,
            _ => MemberRole::Member,
        };
        attempt(SetRole { group, party, role }, refusal, |_| {});
    };
    let selected = move |option: &str| option == held;
    view! {
        <tr>
            <td class="p1">
                {member.display.clone()} {if member.you { " (you)" } else { "" }}
            </td>
            <td class="p1 does">
                <select aria-label="Role" on:change=set>
                    <option value="member" selected=selected("member")>
                        "Member"
                    </option>
                    <option value="admin" selected=selected("admin")>
                        "Admin"
                    </option>
                    <option value="owner" selected=selected("owner")>
                        "Owner"
                    </option>
                </select>
            </td>
            <td class="p2 quiet">{if member.contact { "visible" } else { "—" }}</td>
            <td class="p3 mono num">{when(member.joined_at)}</td>
        </tr>
    }
}

/// Minting an invitation. The token is in the reply and nowhere else, so the
/// claim URL is shown once, here, with the button that carries it.
#[component]
fn Inviting(group: Signal<Option<String>>, refusal: RwSignal<Option<Refusal>>) -> impl IntoView {
    let role = RwSignal::new("member".to_owned());
    let link = RwSignal::new(String::new());

    let invite = Callback::new(move |_: u64| {
        let Some(group) = group.get_untracked() else {
            return;
        };
        let Ok(group) = group.parse() else {
            return;
        };
        let role = match role.get_untracked().as_str() {
            "admin" => MemberRole::Admin,
            _ => MemberRole::Member,
        };
        let expires_at = (js_sys::Date::now() / 1000.0) as i64 + DAY;
        attempt(
            Invite {
                group,
                role,
                expires_at,
            },
            refusal,
            move |result| {
                if let Some(token) = result.get("token").and_then(|token| token.as_str()) {
                    link.set(claim_url(token));
                }
            },
        );
    });

    view! {
        <Section title="Invite somebody">
            <div class="inline">
                <div class="field">
                    <label for="invite-role">"Role"</label>
                    <select
                        id="invite-role"
                        on:change=move |event| role.set(event_target_value(&event))
                    >
                        <option value="member">"Member"</option>
                        <option value="admin">"Admin"</option>
                    </select>
                </div>
                <Commit
                    label="Mint a link"
                    version=Signal::derive(|| 0)
                    on_commit=invite
                    disabled=Signal::derive(move || group.get().is_none())
                />
                <Show when=move || !link.get().is_empty()>
                    <Carry value=Signal::derive(move || link.get()) />
                </Show>
            </div>
        </Section>
    }
}

/// A personal group: no organization, so it is this person's own.
#[component]
fn Making(refusal: RwSignal<Option<Refusal>>) -> impl IntoView {
    let name = RwSignal::new(String::new());
    let create = Callback::new(move |_: u64| {
        attempt(
            CreateGroup {
                display_name: name.get_untracked(),
                organization: None,
            },
            refusal,
            move |_| name.set(String::new()),
        );
    });
    view! {
        <Section title="New group">
            <div class="inline">
                <div class="field">
                    <label for="group-name">"Name"</label>
                    <input
                        id="group-name"
                        type="text"
                        prop:value=move || name.get()
                        on:input=move |event| name.set(event_target_value(&event))
                    />
                    <Note refusal=refusal field="display_name" />
                </div>
                <Commit
                    label="Create group"
                    version=Signal::derive(|| 0)
                    on_commit=create
                    disabled=Signal::derive(move || name.get().trim().is_empty())
                />
            </div>
        </Section>
    }
}

/// The URL a claimant opens. Absolute, because it is going into a message.
fn claim_url(token: &str) -> String {
    let origin = web_sys::window()
        .and_then(|window| window.location().origin().ok())
        .unwrap_or_default();
    format!("{origin}/links/{token}")
}

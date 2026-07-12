//! Viewer resolution for public surfaces. Link capabilities are combined explicitly by handlers.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::state::AppState;
use crate::store::event_links::ResolvedLink;
use crate::store::sessions::SessionContext;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Viewer {
    Anonymous,
    LinkHolder {
        person_id: Option<i64>,
        event_id: i64,
    },
    /// A standing, person-bound calendar capability; unlike an event link it
    /// carries no event-specific direct-hit floor.
    FeedHolder {
        person_id: i64,
    },
    Guest {
        identity_id: i64,
        person_id: i64,
    },
    Owner {
        identity_id: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MismatchNote {
    pub guest_identity_id: i64,
    pub guest_person_id: i64,
}

impl Viewer {
    pub fn person_id(&self) -> Option<i64> {
        match self {
            Self::Guest { person_id, .. }
            | Self::FeedHolder { person_id }
            | Self::LinkHolder {
                person_id: Some(person_id),
                ..
            } => Some(*person_id),
            _ => None,
        }
    }

    /// Applies the binding match table. The link wins over a mismatched guest;
    /// Owner always wins; handlers surface the returned mismatch note.
    pub fn combine_with_link(self, link: Option<&ResolvedLink>) -> (Self, Option<MismatchNote>) {
        let token = link.map(|link| Viewer::LinkHolder {
            person_id: link.person_id,
            event_id: link.event_id,
        });
        match (self, token) {
            (owner @ Viewer::Owner { .. }, _) => (owner, None),
            (
                guest @ Viewer::Guest { person_id, .. },
                Some(Viewer::LinkHolder {
                    person_id: Some(link_person),
                    ..
                }),
            ) if person_id == link_person => (guest, None),
            (
                Viewer::Guest {
                    identity_id,
                    person_id,
                },
                Some(link @ Viewer::LinkHolder { .. }),
            ) => (
                link,
                Some(MismatchNote {
                    guest_identity_id: identity_id,
                    guest_person_id: person_id,
                }),
            ),
            (guest @ Viewer::Guest { .. }, None) => (guest, None),
            (_, Some(link)) => (link, None),
            _ => (Viewer::Anonymous, None),
        }
    }
}

impl FromRequestParts<AppState> for Viewer {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = parts
            .extensions
            .get::<Option<SessionContext>>()
            .cloned()
            .flatten();
        let viewer = match session {
            Some(ctx) if state.owner_account_id() == Some(ctx.account_id) => Viewer::Owner {
                identity_id: ctx.identity_id,
            },
            Some(ctx) => match state.store().active_guest_binding(ctx.identity_id).await {
                Ok(Some(binding)) if Some(binding.owner_account_id) == state.owner_account_id() => {
                    Viewer::Guest {
                        identity_id: ctx.identity_id,
                        person_id: binding.person_id,
                    }
                }
                _ => Viewer::Anonymous,
            },
            None => Viewer::Anonymous,
        };
        Ok(viewer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(person_id: Option<i64>) -> ResolvedLink {
        ResolvedLink {
            id: 1,
            account_id: 1,
            event_id: 8,
            person_id,
            tier: "private".into(),
        }
    }

    #[test]
    fn combine_match_table_is_exhaustive() {
        let owner = Viewer::Owner { identity_id: 1 };
        assert_eq!(
            owner.clone().combine_with_link(Some(&resolved(Some(9)))),
            (owner, None)
        );

        let guest = Viewer::Guest {
            identity_id: 2,
            person_id: 9,
        };
        assert_eq!(
            guest.clone().combine_with_link(Some(&resolved(Some(9)))),
            (guest.clone(), None)
        );
        assert_eq!(guest.clone().combine_with_link(None), (guest.clone(), None));

        let (viewer, note) = guest.combine_with_link(Some(&resolved(Some(10))));
        assert_eq!(
            viewer,
            Viewer::LinkHolder {
                person_id: Some(10),
                event_id: 8
            }
        );
        assert_eq!(
            note,
            Some(MismatchNote {
                guest_identity_id: 2,
                guest_person_id: 9
            })
        );

        assert_eq!(
            Viewer::Anonymous.combine_with_link(Some(&resolved(None))).0,
            Viewer::LinkHolder {
                person_id: None,
                event_id: 8
            }
        );
        assert_eq!(
            Viewer::Anonymous.combine_with_link(None),
            (Viewer::Anonymous, None)
        );
    }
}

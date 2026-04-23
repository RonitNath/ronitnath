use serde::{Deserialize, Serialize};

use crate::auth::SessionData;
use crate::config::AdminConfig;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum Viewer {
    Anonymous,
    Invitee {
        invitee_id: String,
        event_id: String,
    },
    Admin {
        isoastra_identity_id: String,
        role: Option<String>,
    },
}

impl Viewer {
    pub(crate) fn is_admin(&self) -> bool {
        matches!(self, Self::Admin { .. })
    }

    pub(crate) fn admin_identity_id(&self) -> Option<&str> {
        match self {
            Self::Admin {
                isoastra_identity_id,
                ..
            } => Some(isoastra_identity_id),
            Self::Anonymous | Self::Invitee { .. } => None,
        }
    }
}

pub(crate) fn viewer_from_session(admins: &AdminConfig, session: Option<&SessionData>) -> Viewer {
    let Some(session) = session else {
        return Viewer::Anonymous;
    };
    let identity = session.identity_id.to_string();
    if admins
        .identity_ids
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(&identity))
    {
        return Viewer::Admin {
            isoastra_identity_id: identity,
            role: session.role.clone(),
        };
    }
    Viewer::Anonymous
}

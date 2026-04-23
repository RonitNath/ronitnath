use serde::{Deserialize, Serialize};

use crate::events::errors::{EventError, Result};
use crate::events::models::Event;
use crate::events::store::EventStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CapacityDisplay {
    pub(crate) confirmed: i64,
    pub(crate) cap: Option<i64>,
    pub(crate) public_confirmed: i64,
    pub(crate) over_cap: i64,
    pub(crate) self_signup_open: bool,
}

pub(crate) async fn display(store: &EventStore, event: &Event) -> Result<CapacityDisplay> {
    let confirmed = store.confirmed_attendee_count(&event.id).await?;
    let cap = event.attendee_cap;
    let public_confirmed = cap.map_or(confirmed, |limit| confirmed.min(limit));
    let over_cap = cap.map_or(0, |limit| (confirmed - limit).max(0));
    let self_signup_open = cap.is_none_or(|limit| confirmed < limit);
    Ok(CapacityDisplay {
        confirmed,
        cap,
        public_confirmed,
        over_cap,
        self_signup_open,
    })
}

pub(crate) async fn ensure_self_signup_capacity(store: &EventStore, event: &Event) -> Result<()> {
    let cap = display(store, event).await?;
    if cap.self_signup_open {
        Ok(())
    } else {
        Err(EventError::CapacityReached)
    }
}

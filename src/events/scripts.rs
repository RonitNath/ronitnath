use std::collections::BTreeMap;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest as _, Sha256};

use crate::events::errors::{EventError, Result};
use crate::events::models::{Event, Invitee};

#[derive(Debug, Clone)]
pub(crate) struct ScriptContext {
    values: BTreeMap<String, String>,
}

impl ScriptContext {
    pub(crate) fn for_invitee(
        event: &Event,
        invitee: &Invitee,
        rsvp_url: &str,
        signup_url: Option<&str>,
    ) -> Self {
        let mut values = BTreeMap::new();
        values.insert("invitee.name".to_owned(), invitee.display_name.clone());
        values.insert("event.title".to_owned(), event.title.clone());
        values.insert("event.date".to_owned(), event.starts_at.clone());
        let location = if invitee.location_approved {
            event
                .location_name
                .clone()
                .or_else(|| event.approximate_location_name.clone())
        } else {
            event.approximate_location_name.clone()
        };
        values.insert("event.location".to_owned(), location.unwrap_or_default());
        values.insert("rsvp_url".to_owned(), rsvp_url.to_owned());
        values.insert(
            "signup_url".to_owned(),
            signup_url.unwrap_or_default().to_owned(),
        );
        Self { values }
    }

    pub(crate) fn render(&self, template: &str) -> Result<String> {
        let mut out = String::with_capacity(template.len());
        let mut rest = template;
        while let Some(start) = rest.find("{{") {
            let (prefix, tail) = rest.split_at(start);
            out.push_str(prefix);
            let Some(end) = tail.find("}}") else {
                return Err(EventError::InvalidInput(
                    "unclosed script placeholder".to_owned(),
                ));
            };
            let key = tail[2..end].trim();
            let Some(value) = self.values.get(key) else {
                return Err(EventError::InvalidInput(format!(
                    "unknown script placeholder: {key}"
                )));
            };
            out.push_str(value);
            rest = &tail[end + 2..];
        }
        out.push_str(rest);
        Ok(out)
    }
}

pub(crate) fn rendered_hash(rendered: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(rendered.as_bytes()))
}

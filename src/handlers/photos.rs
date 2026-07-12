//! Authz-checked photo upload, serving, and soft deletion routes.

use axum::Extension;
use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Redirect, Response};
use tokio_util::io::ReaderStream;

use crate::auth::extract::GuestScope;
use crate::auth::viewer::Viewer;
use crate::auth::{AccountScope, Role, csrf};
use crate::error::AppError;
use crate::photos::{self, UploadAttribution};
use crate::state::AppState;
use crate::store::event_links::ResolvedLink;
use crate::store::sessions::SessionContext;

#[derive(Debug, Clone)]
pub struct GalleryPhoto {
    pub id: i64,
    pub caption: String,
    pub thumb_url: String,
    pub medium_url: String,
    pub delete_url: String,
    pub can_delete: bool,
}

const MAX_ORIGINAL_FILENAME_BYTES: usize = 255;

struct Upload {
    filename: String,
    caption: String,
    csrf_token: String,
    bytes: Vec<u8>,
}

fn display_filename(value: &str) -> String {
    let basename = value.rsplit(['/', '\\']).next().unwrap_or("photo");
    let mut result = String::new();
    for ch in basename.chars().filter(|ch| !ch.is_control()) {
        if result.len() + ch.len_utf8() > MAX_ORIGINAL_FILENAME_BYTES {
            break;
        }
        result.push(ch);
    }
    if result.is_empty() {
        "photo".into()
    } else {
        result
    }
}

fn multipart_error(error: axum::extract::multipart::MultipartError) -> AppError {
    // Axum reports a tower body-limit stream failure as a generic multipart
    // read error on chunked requests. All extraction failures here occur
    // after the route-local body limiter, so fail with the safe 413 ruling.
    tracing::debug!(%error, "multipart photo extraction failed");
    AppError::PayloadTooLarge
}

async fn multipart(mut multipart: Multipart) -> Result<Upload, AppError> {
    let mut upload = Upload {
        filename: String::new(),
        caption: String::new(),
        csrf_token: String::new(),
        bytes: Vec::new(),
    };
    while let Some(field) = multipart.next_field().await.map_err(multipart_error)? {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "photo" => {
                upload.filename = display_filename(field.file_name().unwrap_or("photo"));
                upload.bytes = field.bytes().await.map_err(multipart_error)?.to_vec();
            }
            "caption" => {
                upload.caption = field
                    .text()
                    .await
                    .map_err(|_| AppError::Invalid("invalid caption".into()))?
            }
            "csrf_token" => {
                upload.csrf_token = field
                    .text()
                    .await
                    .map_err(|_| AppError::Invalid("invalid CSRF token".into()))?
            }
            _ => {}
        }
    }
    if upload.bytes.is_empty() {
        return Err(AppError::Invalid("choose a photo to upload".into()));
    }
    if upload.caption.chars().count() > 500 {
        return Err(AppError::Invalid(
            "caption must be 500 characters or fewer".into(),
        ));
    }
    Ok(upload)
}

async fn token_context(
    state: &AppState,
    token: &str,
    session_viewer: Viewer,
) -> Result<(ResolvedLink, Viewer), AppError> {
    let (link, _event) = crate::handlers::event_public::resolve(state.store(), token).await?;
    let (viewer, _) = session_viewer.combine_with_link(Some(&link));
    ensure_attendee(state, link.account_id, link.event_id, &viewer).await?;
    Ok((link, viewer))
}

async fn ensure_attendee(
    state: &AppState,
    account_id: i64,
    event_id: i64,
    viewer: &Viewer,
) -> Result<(), AppError> {
    if matches!(viewer, Viewer::Owner { .. }) {
        return Ok(());
    }
    let person_id = viewer.person_id().ok_or(AppError::NotFound)?;
    if state
        .store()
        .is_event_attendee(account_id, event_id, person_id)
        .await?
    {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

pub async fn attendee(
    state: &AppState,
    account_id: i64,
    event_id: i64,
    viewer: &Viewer,
) -> Result<bool, AppError> {
    if matches!(viewer, Viewer::Owner { .. }) {
        return Ok(true);
    }
    match viewer.person_id() {
        Some(person_id) => Ok(state
            .store()
            .is_event_attendee(account_id, event_id, person_id)
            .await?),
        None => Ok(false),
    }
}

fn attribution(viewer: &Viewer) -> UploadAttribution {
    match viewer {
        Viewer::Owner { identity_id } | Viewer::Guest { identity_id, .. } => UploadAttribution {
            identity_id: Some(*identity_id),
            person_id: None,
        },
        Viewer::LinkHolder { person_id, .. } => UploadAttribution {
            identity_id: None,
            person_id: *person_id,
        },
        Viewer::Anonymous => UploadAttribution {
            identity_id: None,
            person_id: None,
        },
    }
}

fn verify_session_csrf(session: &Option<SessionContext>, submitted: &str) -> Result<(), AppError> {
    if let Some(session) = session {
        csrf::verify_optional(Some(&session.csrf_token), submitted)?;
    }
    Ok(())
}

async fn save(
    state: &AppState,
    account_id: i64,
    event_id: i64,
    upload_attribution: UploadAttribution,
    upload: Upload,
) -> Result<i64, AppError> {
    let _permit = state.photo_ingest_permit().await;
    let max_pixels = state.photo_max_pixels();
    let max_side = state.photo_max_side();
    let bytes = upload.bytes;
    let processed = tokio::task::spawn_blocking(move || {
        photos::process_with_limits(&bytes, max_pixels, max_side)
    })
    .await
    .map_err(|error| anyhow::anyhow!("photo processing task failed: {error}"))??;
    photos::persist(
        state.store(),
        state.photo_storage_dir(),
        account_id,
        event_id,
        &upload.filename,
        upload.caption.trim(),
        upload_attribution,
        processed,
    )
    .await
}

pub async fn upload_token(
    State(state): State<AppState>,
    viewer: Viewer,
    Extension(session): Extension<Option<SessionContext>>,
    Path(token): Path<String>,
    multipart_body: Multipart,
) -> Result<Response, AppError> {
    let (link, actor) = token_context(&state, &token, viewer).await?;
    let upload = multipart(multipart_body).await?;
    verify_session_csrf(&session, &upload.csrf_token)?;
    let upload_attribution = session.as_ref().map_or_else(
        || attribution(&actor),
        |session| UploadAttribution {
            identity_id: Some(session.identity_id),
            person_id: None,
        },
    );
    save(
        &state,
        link.account_id,
        link.event_id,
        upload_attribution,
        upload,
    )
    .await?;
    Ok(Redirect::to(&format!("/e/{token}#photos")).into_response())
}

pub async fn upload_my(
    State(state): State<AppState>,
    scope: GuestScope,
    Path(event_id): Path<i64>,
    multipart_body: Multipart,
) -> Result<Response, AppError> {
    let actor = Viewer::Guest {
        identity_id: scope.identity_id,
        person_id: scope.person_id,
    };
    ensure_attendee(&state, scope.owner_account_id, event_id, &actor).await?;
    let upload = multipart(multipart_body).await?;
    csrf::verify_optional(Some(&scope.csrf_token), &upload.csrf_token)?;
    save(
        &state,
        scope.owner_account_id,
        event_id,
        attribution(&actor),
        upload,
    )
    .await?;
    Ok(Redirect::to(&format!("/my/events/{event_id}#photos")).into_response())
}

pub async fn upload_admin(
    State(state): State<AppState>,
    scope: AccountScope,
    Path(event_id): Path<i64>,
    multipart_body: Multipart,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    state
        .store()
        .find_event(scope.account_id, event_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let upload = multipart(multipart_body).await?;
    csrf::verify(&scope, &upload.csrf_token)?;
    let actor = Viewer::Owner {
        identity_id: scope.identity_id,
    };
    save(
        &state,
        scope.account_id,
        event_id,
        attribution(&actor),
        upload,
    )
    .await?;
    Ok(Redirect::to(&format!("/events/{event_id}#photos")).into_response())
}

fn variant(value: &str) -> Result<&str, AppError> {
    match value {
        "original" | "thumb" | "medium" => Ok(value),
        _ => Err(AppError::NotFound),
    }
}

async fn stream(
    state: &AppState,
    account_id: i64,
    event_id: i64,
    photo_id: i64,
    kind: &str,
    viewer: &Viewer,
) -> Result<Response, AppError> {
    ensure_attendee(state, account_id, event_id, viewer).await?;
    let row = state
        .store()
        .find_photo_variant_for_viewer(
            account_id,
            event_id,
            photo_id,
            variant(kind)?,
            viewer.person_id(),
            matches!(viewer, Viewer::Owner { .. }),
        )
        .await?
        .ok_or(AppError::NotFound)?;
    let path =
        photos::event_dir(state.photo_storage_dir(), account_id, event_id).join(row.storage_key);
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|_| AppError::NotFound)?;
    let mut response = Body::from_stream(ReaderStream::new(file)).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("image/webp"));
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, immutable"),
    );
    Ok(response)
}

pub async fn serve_token(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((token, photo_id, kind)): Path<(String, i64, String)>,
) -> Result<Response, AppError> {
    let (link, actor) = token_context(&state, &token, viewer).await?;
    stream(
        &state,
        link.account_id,
        link.event_id,
        photo_id,
        &kind,
        &actor,
    )
    .await
}

pub async fn serve_admin(
    State(state): State<AppState>,
    scope: AccountScope,
    Path((event_id, photo_id, kind)): Path<(i64, i64, String)>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    let actor = Viewer::Owner {
        identity_id: scope.identity_id,
    };
    stream(&state, scope.account_id, event_id, photo_id, &kind, &actor).await
}

pub async fn serve_my(
    State(state): State<AppState>,
    scope: GuestScope,
    Path((event_id, photo_id, kind)): Path<(i64, i64, String)>,
) -> Result<Response, AppError> {
    let actor = Viewer::Guest {
        identity_id: scope.identity_id,
        person_id: scope.person_id,
    };
    stream(
        &state,
        scope.owner_account_id,
        event_id,
        photo_id,
        &kind,
        &actor,
    )
    .await
}

async fn delete(
    state: &AppState,
    account_id: i64,
    event_id: i64,
    photo_id: i64,
    actor: &Viewer,
) -> Result<(), AppError> {
    ensure_attendee(state, account_id, event_id, actor).await?;
    let (identity, person, owner) = match actor {
        Viewer::Owner { identity_id } => (Some(*identity_id), None, true),
        Viewer::Guest { identity_id, .. } => (Some(*identity_id), None, false),
        Viewer::LinkHolder { person_id, .. } => (None, *person_id, false),
        Viewer::Anonymous => (None, None, false),
    };
    if state
        .store()
        .soft_delete_photo(account_id, event_id, photo_id, identity, person, owner)
        .await?
        == 0
    {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub async fn delete_token(
    State(state): State<AppState>,
    viewer: Viewer,
    Extension(session): Extension<Option<SessionContext>>,
    Path((token, photo_id)): Path<(String, i64)>,
    mut multipart_body: Multipart,
) -> Result<Response, AppError> {
    let (link, actor) = token_context(&state, &token, viewer).await?;
    let upload = multipart_fields(&mut multipart_body).await?;
    verify_session_csrf(&session, &upload)?;
    delete(&state, link.account_id, link.event_id, photo_id, &actor).await?;
    Ok(Redirect::to(&format!("/e/{token}#photos")).into_response())
}

pub async fn delete_my(
    State(state): State<AppState>,
    scope: GuestScope,
    Path((event_id, photo_id)): Path<(i64, i64)>,
    mut multipart_body: Multipart,
) -> Result<Response, AppError> {
    let csrf_token = multipart_fields(&mut multipart_body).await?;
    csrf::verify_optional(Some(&scope.csrf_token), &csrf_token)?;
    let actor = Viewer::Guest {
        identity_id: scope.identity_id,
        person_id: scope.person_id,
    };
    delete(&state, scope.owner_account_id, event_id, photo_id, &actor).await?;
    Ok(Redirect::to(&format!("/my/events/{event_id}#photos")).into_response())
}

pub async fn delete_admin(
    State(state): State<AppState>,
    scope: AccountScope,
    Path((event_id, photo_id)): Path<(i64, i64)>,
    mut multipart_body: Multipart,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    let token = multipart_fields(&mut multipart_body).await?;
    csrf::verify(&scope, &token)?;
    let actor = Viewer::Owner {
        identity_id: scope.identity_id,
    };
    delete(&state, scope.account_id, event_id, photo_id, &actor).await?;
    Ok(Redirect::to(&format!("/events/{event_id}#photos")).into_response())
}

async fn multipart_fields(multipart: &mut Multipart) -> Result<String, AppError> {
    while let Some(field) = multipart.next_field().await.map_err(multipart_error)? {
        if field.name() == Some("csrf_token") {
            return field
                .text()
                .await
                .map_err(|_| AppError::Invalid("invalid CSRF token".into()));
        }
    }
    Ok(String::new())
}

pub async fn gallery(
    state: &AppState,
    account_id: i64,
    event_id: i64,
    viewer: &Viewer,
    prefix: &str,
) -> Result<Vec<GalleryPhoto>, AppError> {
    let photos = state
        .store()
        .list_photos_for_viewer(
            account_id,
            event_id,
            viewer.person_id(),
            matches!(viewer, Viewer::Owner { .. }),
        )
        .await?;
    Ok(photos
        .into_iter()
        .map(|p| {
            let can_delete = matches!(viewer, Viewer::Owner { .. })
                || viewer_identity(viewer) == p.uploaded_by_identity_id
                || (viewer_identity(viewer).is_none()
                    && viewer.person_id() == p.uploaded_by_person_id);
            GalleryPhoto {
                id: p.id,
                caption: p.caption,
                thumb_url: format!("{prefix}/{}/thumb", p.id),
                medium_url: format!("{prefix}/{}/medium", p.id),
                delete_url: format!("{prefix}/{}/delete", p.id),
                can_delete,
            }
        })
        .collect())
}

fn viewer_identity(viewer: &Viewer) -> Option<i64> {
    match viewer {
        Viewer::Owner { identity_id } | Viewer::Guest { identity_id, .. } => Some(*identity_id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_filename_is_a_control_free_bounded_display_basename() {
        let sanitized = display_filename(&format!("../../folder\\evil\n{}😀.jpg", "a".repeat(300)));
        assert!(!sanitized.contains(['/', '\\', '\n']));
        assert!(sanitized.len() <= MAX_ORIGINAL_FILENAME_BYTES);
        assert!(std::str::from_utf8(sanitized.as_bytes()).is_ok());
        assert_eq!(display_filename("../\0"), "photo");
    }
}

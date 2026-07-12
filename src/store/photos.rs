//! Account-scoped persistence for event photos and their immutable variants.

use super::Store;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Photo {
    pub id: i64,
    pub account_id: i64,
    pub event_id: i64,
    pub uploaded_by_identity_id: Option<i64>,
    pub uploaded_by_person_id: Option<i64>,
    pub storage_key: String,
    pub original_filename: String,
    pub mime_type: String,
    pub byte_size: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub caption: String,
    pub taken_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PhotoVariant {
    pub kind: String,
    pub storage_key: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub byte_size: i64,
}

pub struct NewPhoto<'a> {
    pub account_id: i64,
    pub event_id: i64,
    pub uploaded_by_identity_id: Option<i64>,
    pub uploaded_by_person_id: Option<i64>,
    pub storage_key: &'a str,
    pub original_filename: &'a str,
    pub mime_type: &'a str,
    pub byte_size: i64,
    pub width: i64,
    pub height: i64,
    pub caption: &'a str,
    pub taken_at: Option<&'a str>,
}

impl Store {
    /// Photo visibility chokepoint: owner, or the viewer's going/attended row.
    pub async fn list_photos_for_viewer(
        &self,
        account_id: i64,
        event_id: i64,
        person_id: Option<i64>,
        owner: bool,
    ) -> sqlx::Result<Vec<Photo>> {
        let owner = i64::from(owner);
        sqlx::query_as!(
            Photo,
            r#"SELECT id as "id!: i64", account_id as "account_id!: i64", event_id as "event_id!: i64",
                      uploaded_by_identity_id, uploaded_by_person_id, storage_key, original_filename,
                      mime_type, byte_size as "byte_size!: i64", width, height, caption, taken_at, created_at
               FROM photos
               WHERE account_id = ?1 AND event_id = ?2 AND deleted_at IS NULL
                 AND (?4 = 1 OR EXISTS (
                     SELECT 1 FROM attendance a
                     WHERE a.account_id = ?1 AND a.event_id = ?2 AND a.person_id = ?3
                       AND a.status IN ('going', 'attended')
                 ))
               ORDER BY COALESCE(taken_at, created_at), id"#,
            account_id,
            event_id,
            person_id,
            owner,
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn list_photos_admin(
        &self,
        account_id: i64,
        event_id: i64,
    ) -> sqlx::Result<Vec<Photo>> {
        self.list_photos_for_viewer(account_id, event_id, None, true)
            .await
    }

    pub async fn find_photo_variant_for_viewer(
        &self,
        account_id: i64,
        event_id: i64,
        photo_id: i64,
        kind: &str,
        person_id: Option<i64>,
        owner: bool,
    ) -> sqlx::Result<Option<PhotoVariant>> {
        let owner = i64::from(owner);
        sqlx::query_as!(
            PhotoVariant,
            r#"SELECT v.kind, v.storage_key, v.width, v.height, v.byte_size as "byte_size!: i64"
               FROM photo_variants v
               JOIN photos p ON p.account_id = v.account_id AND p.id = v.photo_id
               WHERE p.account_id = ?1 AND p.event_id = ?2 AND p.id = ?3 AND v.kind = ?4
                 AND p.deleted_at IS NULL
                 AND (?6 = 1 OR EXISTS (
                     SELECT 1 FROM attendance a
                     WHERE a.account_id = ?1 AND a.event_id = ?2 AND a.person_id = ?5
                       AND a.status IN ('going', 'attended')
                 ))"#,
            account_id,
            event_id,
            photo_id,
            kind,
            person_id,
            owner,
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn insert_photo_with_variants(
        &self,
        photo: &NewPhoto<'_>,
        variants: &[PhotoVariant],
    ) -> sqlx::Result<i64> {
        let mut tx = self.pool.begin().await?;
        let id = sqlx::query_scalar!(
            r#"INSERT INTO photos
               (account_id, event_id, uploaded_by_identity_id, uploaded_by_person_id, storage_key,
                original_filename, mime_type, byte_size, width, height, caption, taken_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
               RETURNING id as "id!: i64""#,
            photo.account_id,
            photo.event_id,
            photo.uploaded_by_identity_id,
            photo.uploaded_by_person_id,
            photo.storage_key,
            photo.original_filename,
            photo.mime_type,
            photo.byte_size,
            photo.width,
            photo.height,
            photo.caption,
            photo.taken_at,
        )
        .fetch_one(&mut *tx)
        .await?;
        for variant in variants {
            sqlx::query!(
                r#"INSERT INTO photo_variants (account_id, photo_id, kind, storage_key, width, height, byte_size)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
                photo.account_id,
                id,
                variant.kind,
                variant.storage_key,
                variant.width,
                variant.height,
                variant.byte_size,
            )
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(id)
    }

    pub async fn soft_delete_photo(
        &self,
        account_id: i64,
        event_id: i64,
        photo_id: i64,
        identity_id: Option<i64>,
        person_id: Option<i64>,
        owner: bool,
    ) -> sqlx::Result<u64> {
        let owner = i64::from(owner);
        Ok(sqlx::query!(
            r#"UPDATE photos SET deleted_at = datetime('now')
               WHERE account_id = ?1 AND event_id = ?2 AND id = ?3 AND deleted_at IS NULL
                 AND (?6 = 1 OR uploaded_by_identity_id = ?4 OR uploaded_by_person_id = ?5)"#,
            account_id,
            event_id,
            photo_id,
            identity_id,
            person_id,
            owner,
        )
        .execute(&self.pool)
        .await?
        .rows_affected())
    }

    pub async fn gc_photo_candidates(
        &self,
        account_id: i64,
        older_than_days: i64,
    ) -> sqlx::Result<Vec<Photo>> {
        sqlx::query_as!(
            Photo,
            r#"SELECT id as "id!: i64", account_id as "account_id!: i64", event_id as "event_id!: i64",
                      uploaded_by_identity_id, uploaded_by_person_id, storage_key, original_filename,
                      mime_type, byte_size as "byte_size!: i64", width, height, caption, taken_at, created_at
               FROM photos WHERE account_id = ?1 AND deleted_at IS NOT NULL
                 AND deleted_at <= datetime('now', '-' || ?2 || ' days')"#,
            account_id,
            older_than_days,
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn storage_key_has_live_photo(
        &self,
        account_id: i64,
        event_id: i64,
        storage_key: &str,
    ) -> sqlx::Result<bool> {
        Ok(sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM photos
               WHERE account_id = ?1 AND event_id = ?2 AND storage_key = ?3 AND deleted_at IS NULL) as "exists!: bool""#,
            account_id, event_id, storage_key
        ).fetch_one(&self.pool).await?)
    }

    #[cfg(test)]
    pub async fn list_photo_variants(
        &self,
        account_id: i64,
        photo_id: i64,
    ) -> sqlx::Result<Vec<PhotoVariant>> {
        sqlx::query_as!(
            PhotoVariant,
            r#"SELECT kind, storage_key, width, height, byte_size as "byte_size!: i64"
               FROM photo_variants WHERE account_id = ?1 AND photo_id = ?2 ORDER BY kind"#,
            account_id,
            photo_id
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn purge_photo_row(&self, account_id: i64, photo_id: i64) -> sqlx::Result<u64> {
        let mut tx = self.pool.begin().await?;
        sqlx::query!(
            "DELETE FROM photo_variants WHERE account_id = ?1 AND photo_id = ?2",
            account_id,
            photo_id
        )
        .execute(&mut *tx)
        .await?;
        let n = sqlx::query!(
            "DELETE FROM photos WHERE account_id = ?1 AND id = ?2 AND deleted_at IS NOT NULL",
            account_id,
            photo_id
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;
        Ok(n)
    }
}

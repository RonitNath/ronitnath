//! Queries and types for the `people` table — the longitudinal person
//! registry. Account-scoped like every domain table (see guestbook.rs).

use serde::Serialize;
use ts_rs::TS;
use utoipa::ToSchema;

use super::Store;

#[derive(Debug, Serialize, sqlx::FromRow, TS, ToSchema)]
#[ts(export)]
pub struct Person {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    pub nickname: String,
    pub group_label: String,
    pub contact: String,
    pub notes: String,
    pub created_at: String,
}

/// A person plus their cross-event attendance history — the longitudinal
/// view backing `/people`.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PersonHistory {
    pub id: i64,
    pub name: String,
    pub nickname: String,
    pub group_label: String,
    pub events_attended: i64,
    pub events_list: String,
}

impl Store {
    pub async fn list_people(&self, account_id: i64) -> sqlx::Result<Vec<Person>> {
        sqlx::query_as!(
            Person,
            r#"SELECT id as "id!: i64", name, nickname, group_label, contact, notes, created_at
               FROM people WHERE account_id = ?1 ORDER BY name COLLATE NOCASE"#,
            account_id,
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn create_person(
        &self,
        account_id: i64,
        name: &str,
        group_label: &str,
    ) -> sqlx::Result<Person> {
        sqlx::query_as!(
            Person,
            r#"INSERT INTO people (account_id, name, group_label)
               VALUES (?1, ?2, ?3)
               RETURNING id as "id!: i64", name, nickname, group_label, contact, notes, created_at"#,
            account_id,
            name,
            group_label,
        )
        .fetch_one(&self.pool)
        .await
    }

    pub async fn find_person(
        &self,
        account_id: i64,
        person_id: i64,
    ) -> sqlx::Result<Option<Person>> {
        sqlx::query_as!(
            Person,
            r#"SELECT id as "id!: i64", name, nickname, group_label, contact, notes, created_at
               FROM people WHERE account_id = ?1 AND id = ?2"#,
            account_id,
            person_id,
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn find_person_by_name(
        &self,
        account_id: i64,
        name: &str,
    ) -> sqlx::Result<Option<Person>> {
        sqlx::query_as!(
            Person,
            r#"SELECT id as "id!: i64", name, nickname, group_label, contact, notes, created_at
               FROM people WHERE account_id = ?1 AND name = ?2 COLLATE NOCASE"#,
            account_id,
            name,
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn update_person(
        &self,
        account_id: i64,
        person_id: i64,
        name: &str,
        nickname: &str,
    ) -> sqlx::Result<u64> {
        let result = sqlx::query!(
            r#"UPDATE people
               SET name = ?3, nickname = ?4
               WHERE account_id = ?1 AND id = ?2"#,
            account_id,
            person_id,
            name,
            nickname,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// People with how many events they've been part of — the reason this
    /// platform keeps one `people` table across events instead of a fresh
    /// guest list per event.
    pub async fn list_people_with_history(
        &self,
        account_id: i64,
    ) -> sqlx::Result<Vec<PersonHistory>> {
        sqlx::query_as!(
            PersonHistory,
            r#"SELECT p.id as "id!: i64", p.name, p.nickname, p.group_label,
                      COUNT(a.id) as "events_attended!: i64",
                      COALESCE(GROUP_CONCAT(e.title, ' · '), '') as "events_list!: String"
               FROM people p
               LEFT JOIN attendance a
                      ON a.account_id = p.account_id
                     AND a.person_id = p.id
                     AND a.status IN ('going', 'attended')
               LEFT JOIN events e ON e.account_id = p.account_id AND e.id = a.event_id
               WHERE p.account_id = ?1
               GROUP BY p.id
               ORDER BY p.name COLLATE NOCASE"#,
            account_id,
        )
        .fetch_all(&self.pool)
        .await
    }
}

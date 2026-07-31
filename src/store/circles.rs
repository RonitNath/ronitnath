//! Account-scoped circle and membership persistence.

use super::Store;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Circle {
    pub id: i64,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CircleMember {
    pub person_id: i64,
    pub person_name: String,
}

impl Store {
    pub async fn list_circles(&self, account_id: i64) -> sqlx::Result<Vec<Circle>> {
        sqlx::query_as!(Circle, r#"SELECT id as "id!: i64", name, created_at FROM circles WHERE account_id = ?1 ORDER BY name COLLATE NOCASE"#, account_id)
            .fetch_all(&self.pool).await
    }

    pub async fn find_circle(
        &self,
        account_id: i64,
        circle_id: i64,
    ) -> sqlx::Result<Option<Circle>> {
        sqlx::query_as!(Circle, r#"SELECT id as "id!: i64", name, created_at FROM circles WHERE account_id = ?1 AND id = ?2"#, account_id, circle_id)
            .fetch_optional(&self.pool).await
    }

    pub async fn find_circle_by_name(
        &self,
        account_id: i64,
        name: &str,
    ) -> sqlx::Result<Option<Circle>> {
        sqlx::query_as!(Circle, r#"SELECT id as "id!: i64", name, created_at FROM circles WHERE account_id = ?1 AND name = ?2 COLLATE NOCASE"#, account_id, name)
            .fetch_optional(&self.pool).await
    }

    pub async fn create_circle(&self, account_id: i64, name: &str) -> sqlx::Result<i64> {
        sqlx::query_scalar!(
            r#"INSERT INTO circles (account_id, name) VALUES (?1, ?2) RETURNING id as "id!: i64""#,
            account_id,
            name
        )
        .fetch_one(&self.pool)
        .await
    }

    pub async fn rename_circle(
        &self,
        account_id: i64,
        circle_id: i64,
        name: &str,
    ) -> sqlx::Result<u64> {
        Ok(sqlx::query!(
            "UPDATE circles SET name = ?3 WHERE account_id = ?1 AND id = ?2",
            account_id,
            circle_id,
            name
        )
        .execute(&self.pool)
        .await?
        .rows_affected())
    }

    pub async fn delete_circle(&self, account_id: i64, circle_id: i64) -> sqlx::Result<u64> {
        Ok(sqlx::query!(
            "DELETE FROM circles WHERE account_id = ?1 AND id = ?2",
            account_id,
            circle_id
        )
        .execute(&self.pool)
        .await?
        .rows_affected())
    }

    pub async fn list_circle_members(
        &self,
        account_id: i64,
        circle_id: i64,
    ) -> sqlx::Result<Vec<CircleMember>> {
        sqlx::query_as!(CircleMember, r#"SELECT cm.person_id as "person_id!: i64", p.name as person_name
            FROM circle_members cm JOIN people p ON p.account_id = cm.account_id AND p.id = cm.person_id
            WHERE cm.account_id = ?1 AND cm.circle_id = ?2 ORDER BY p.name COLLATE NOCASE"#, account_id, circle_id)
            .fetch_all(&self.pool).await
    }

    pub async fn add_circle_member(
        &self,
        account_id: i64,
        circle_id: i64,
        person_id: i64,
    ) -> sqlx::Result<u64> {
        Ok(sqlx::query!(
            r#"INSERT INTO circle_members (account_id, circle_id, person_id)
            SELECT ?1, c.id, p.id FROM circles c JOIN people p ON p.account_id = c.account_id
            WHERE c.account_id = ?1 AND c.id = ?2 AND p.id = ?3"#,
            account_id,
            circle_id,
            person_id
        )
        .execute(&self.pool)
        .await?
        .rows_affected())
    }

    pub async fn remove_circle_member(
        &self,
        account_id: i64,
        circle_id: i64,
        person_id: i64,
    ) -> sqlx::Result<u64> {
        Ok(sqlx::query!("DELETE FROM circle_members WHERE account_id = ?1 AND circle_id = ?2 AND person_id = ?3", account_id, circle_id, person_id)
            .execute(&self.pool).await?.rows_affected())
    }
}

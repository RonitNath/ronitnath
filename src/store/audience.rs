//! Account-scoped audience policy persistence. Level math stays in `access::level`.

use super::Store;
use crate::access::level::{
    self, AudiencePolicy, CircleGrant, Level, OverrideKind, PersonOverride,
};
use crate::auth::viewer::Viewer;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AudiencePolicyRow {
    pub id: i64,
    pub public_level: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CircleGrantRow {
    pub circle_id: i64,
    pub circle_name: String,
    pub level: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PersonOverrideRow {
    pub person_id: i64,
    pub person_name: String,
    pub override_kind: String,
    pub level: Option<String>,
}

#[derive(Debug)]
pub struct AudienceUpdate {
    pub public_level: String,
    pub circles: Vec<(i64, Option<String>)>,
    pub people: Vec<(i64, Option<String>, Option<String>)>,
}

#[derive(Debug)]
pub struct AudienceInputs {
    pub policy: AudiencePolicyRow,
    pub overrides: Vec<PersonOverrideRow>,
    pub circle_grants: Vec<CircleGrantRow>,
    pub person_circles: Vec<i64>,
}

impl AudienceInputs {
    fn parsed(&self) -> anyhow::Result<(AudiencePolicy, Vec<PersonOverride>, Vec<CircleGrant>)> {
        let policy = AudiencePolicy {
            public_level: self
                .policy
                .public_level
                .parse()
                .map_err(anyhow::Error::msg)?,
        };
        let overrides = self
            .overrides
            .iter()
            .map(|row| {
                let kind = match row.override_kind.as_str() {
                    "include" => OverrideKind::Include,
                    "exclude" => OverrideKind::Exclude,
                    _ => anyhow::bail!("invalid persisted person override kind"),
                };
                Ok(PersonOverride {
                    person_id: row.person_id,
                    kind,
                    level: row
                        .level
                        .as_deref()
                        .map(str::parse)
                        .transpose()
                        .map_err(anyhow::Error::msg)?,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let grants = self
            .circle_grants
            .iter()
            .map(|row| {
                Ok(CircleGrant {
                    circle_id: row.circle_id,
                    level: row.level.parse().map_err(anyhow::Error::msg)?,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok((policy, overrides, grants))
    }

    pub fn level_for(&self, viewer: &Viewer) -> anyhow::Result<Level> {
        let (policy, overrides, grants) = self.parsed()?;
        Ok(level::level_for(
            viewer,
            &policy,
            &overrides,
            &grants,
            &self.person_circles,
        ))
    }

    pub fn level_for_direct_hit(&self, viewer: &Viewer, link_tier: &str) -> anyhow::Result<Level> {
        let (policy, overrides, grants) = self.parsed()?;
        Ok(level::level_for_direct_hit(
            viewer,
            link_tier,
            &policy,
            &overrides,
            &grants,
            &self.person_circles,
        ))
    }
}

impl Store {
    pub async fn find_audience_policy(
        &self,
        account_id: i64,
        subject_type: &str,
        subject_id: i64,
    ) -> sqlx::Result<Option<AudiencePolicyRow>> {
        sqlx::query_as!(AudiencePolicyRow, r#"SELECT id as "id!: i64", public_level
            FROM audience_policies WHERE account_id = ?1 AND subject_type = ?2 AND subject_id = ?3"#,
            account_id, subject_type, subject_id)
            .fetch_optional(&self.pool).await
    }

    pub async fn audience_inputs_for_event(
        &self,
        account_id: i64,
        event_id: i64,
        person_id: Option<i64>,
    ) -> sqlx::Result<Option<AudienceInputs>> {
        self.audience_inputs(account_id, "event", event_id, person_id)
            .await
    }

    pub async fn audience_inputs_for_calendar_entry(
        &self,
        account_id: i64,
        entry_id: i64,
        person_id: Option<i64>,
    ) -> sqlx::Result<Option<AudienceInputs>> {
        self.audience_inputs(account_id, "calendar_entry", entry_id, person_id)
            .await
    }

    async fn audience_inputs(
        &self,
        account_id: i64,
        subject_type: &str,
        subject_id: i64,
        person_id: Option<i64>,
    ) -> sqlx::Result<Option<AudienceInputs>> {
        let Some(policy) = self
            .find_audience_policy(account_id, subject_type, subject_id)
            .await?
        else {
            return Ok(None);
        };
        let overrides = if let Some(person_id) = person_id {
            sqlx::query_as!(
                PersonOverrideRow,
                r#"SELECT o.person_id as "person_id!: i64",
                    p.name as person_name, o.override_kind, o.level
                FROM audience_person_overrides o
                JOIN people p ON p.account_id = o.account_id AND p.id = o.person_id
                WHERE o.account_id = ?1 AND o.policy_id = ?2 AND o.person_id = ?3"#,
                account_id,
                policy.id,
                person_id
            )
            .fetch_all(&self.pool)
            .await?
        } else {
            Vec::new()
        };
        let circle_grants = sqlx::query_as!(
            CircleGrantRow,
            r#"SELECT g.circle_id as "circle_id!: i64",
                c.name as circle_name, g.level
            FROM audience_circle_grants g
            JOIN circles c ON c.account_id = g.account_id AND c.id = g.circle_id
            WHERE g.account_id = ?1 AND g.policy_id = ?2"#,
            account_id,
            policy.id
        )
        .fetch_all(&self.pool)
        .await?;
        let person_circles = if let Some(person_id) = person_id {
            sqlx::query_scalar!(
                r#"SELECT circle_id as "circle_id!: i64" FROM circle_members
                WHERE account_id = ?1 AND person_id = ?2"#,
                account_id,
                person_id
            )
            .fetch_all(&self.pool)
            .await?
        } else {
            Vec::new()
        };
        Ok(Some(AudienceInputs {
            policy,
            overrides,
            circle_grants,
            person_circles,
        }))
    }

    pub async fn list_audience_overrides(
        &self,
        account_id: i64,
        policy_id: i64,
    ) -> sqlx::Result<Vec<PersonOverrideRow>> {
        sqlx::query_as!(
            PersonOverrideRow,
            r#"SELECT o.person_id as "person_id!: i64",
                p.name as person_name, o.override_kind, o.level
            FROM audience_person_overrides o
            JOIN people p ON p.account_id = o.account_id AND p.id = o.person_id
            WHERE o.account_id = ?1 AND o.policy_id = ?2 ORDER BY p.name COLLATE NOCASE"#,
            account_id,
            policy_id
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn list_audience_grants(
        &self,
        account_id: i64,
        policy_id: i64,
    ) -> sqlx::Result<Vec<CircleGrantRow>> {
        sqlx::query_as!(
            CircleGrantRow,
            r#"SELECT g.circle_id as "circle_id!: i64",
                c.name as circle_name, g.level
            FROM audience_circle_grants g
            JOIN circles c ON c.account_id = g.account_id AND c.id = g.circle_id
            WHERE g.account_id = ?1 AND g.policy_id = ?2 ORDER BY c.name COLLATE NOCASE"#,
            account_id,
            policy_id
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn set_public_level(
        &self,
        account_id: i64,
        policy_id: i64,
        level: &str,
    ) -> sqlx::Result<u64> {
        Ok(sqlx::query!(
            "UPDATE audience_policies SET public_level = ?3 WHERE account_id = ?1 AND id = ?2",
            account_id,
            policy_id,
            level
        )
        .execute(&self.pool)
        .await?
        .rows_affected())
    }

    pub async fn set_circle_grant(
        &self,
        account_id: i64,
        policy_id: i64,
        circle_id: i64,
        level: Option<&str>,
    ) -> sqlx::Result<()> {
        if let Some(level) = level {
            sqlx::query!(
                r#"INSERT INTO audience_circle_grants (account_id, policy_id, circle_id, level)
                SELECT ?1, p.id, c.id, ?4 FROM audience_policies p
                JOIN circles c ON c.account_id = p.account_id
                WHERE p.account_id = ?1 AND p.id = ?2 AND c.id = ?3
                ON CONFLICT(policy_id, circle_id) DO UPDATE SET level = excluded.level"#,
                account_id,
                policy_id,
                circle_id,
                level
            )
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query!("DELETE FROM audience_circle_grants WHERE account_id = ?1 AND policy_id = ?2 AND circle_id = ?3",
                account_id, policy_id, circle_id).execute(&self.pool).await?;
        }
        Ok(())
    }

    /// Applies a fully validated editor submission and its audit record as
    /// one transaction, so malformed forms can never partially broaden access.
    #[allow(clippy::too_many_arguments)]
    pub async fn apply_audience_update(
        &self,
        account_id: i64,
        policy_id: i64,
        identity_id: i64,
        subject_type: &str,
        subject_id: i64,
        update: &AudienceUpdate,
    ) -> sqlx::Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query!(
            "UPDATE audience_policies SET public_level = ?3 WHERE account_id = ?1 AND id = ?2",
            account_id,
            policy_id,
            update.public_level
        )
        .execute(&mut *tx)
        .await?;
        for (circle_id, level) in &update.circles {
            if let Some(level) = level {
                sqlx::query!(
                    r#"INSERT INTO audience_circle_grants (account_id, policy_id, circle_id, level)
                    SELECT ?1, p.id, c.id, ?4 FROM audience_policies p
                    JOIN circles c ON c.account_id = p.account_id
                    WHERE p.account_id = ?1 AND p.id = ?2 AND c.id = ?3
                    ON CONFLICT(policy_id, circle_id) DO UPDATE SET level = excluded.level"#,
                    account_id,
                    policy_id,
                    circle_id,
                    level
                )
                .execute(&mut *tx)
                .await?;
            } else {
                sqlx::query!("DELETE FROM audience_circle_grants WHERE account_id = ?1 AND policy_id = ?2 AND circle_id = ?3",
                    account_id, policy_id, circle_id).execute(&mut *tx).await?;
            }
        }
        for (person_id, kind, level) in &update.people {
            if let Some(kind) = kind {
                sqlx::query!(
                    r#"INSERT INTO audience_person_overrides
                        (account_id, policy_id, person_id, override_kind, level)
                    SELECT ?1, p.id, person.id, ?4, ?5 FROM audience_policies p
                    JOIN people person ON person.account_id = p.account_id
                    WHERE p.account_id = ?1 AND p.id = ?2 AND person.id = ?3
                    ON CONFLICT(policy_id, person_id) DO UPDATE SET
                        override_kind = excluded.override_kind, level = excluded.level"#,
                    account_id,
                    policy_id,
                    person_id,
                    kind,
                    level
                )
                .execute(&mut *tx)
                .await?;
            } else {
                sqlx::query!("DELETE FROM audience_person_overrides WHERE account_id = ?1 AND policy_id = ?2 AND person_id = ?3",
                    account_id, policy_id, person_id).execute(&mut *tx).await?;
            }
        }
        let entity_id = subject_id.to_string();
        let detail = serde_json::json!({"public_level": update.public_level}).to_string();
        sqlx::query!(
            r#"INSERT INTO audit_log (identity_id, account_id, request_id, action, entity, entity_id, detail)
               VALUES (?1, ?2, NULL, 'audience.updated', ?3, ?4, ?5)"#,
            identity_id,
            account_id,
            subject_type,
            entity_id,
            detail,
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await
    }

    pub async fn set_person_override(
        &self,
        account_id: i64,
        policy_id: i64,
        person_id: i64,
        kind: Option<&str>,
        level: Option<&str>,
    ) -> sqlx::Result<()> {
        if let Some(kind) = kind {
            sqlx::query!(
                r#"INSERT INTO audience_person_overrides
                    (account_id, policy_id, person_id, override_kind, level)
                SELECT ?1, p.id, person.id, ?4, ?5 FROM audience_policies p
                JOIN people person ON person.account_id = p.account_id
                WHERE p.account_id = ?1 AND p.id = ?2 AND person.id = ?3
                ON CONFLICT(policy_id, person_id) DO UPDATE SET
                    override_kind = excluded.override_kind, level = excluded.level"#,
                account_id,
                policy_id,
                person_id,
                kind,
                level
            )
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query!("DELETE FROM audience_person_overrides WHERE account_id = ?1 AND policy_id = ?2 AND person_id = ?3",
                account_id, policy_id, person_id).execute(&self.pool).await?;
        }
        Ok(())
    }
}

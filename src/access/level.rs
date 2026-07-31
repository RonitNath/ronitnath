//! Pure visibility-level computation. No database or rendering logic belongs here.

use std::str::FromStr;

use crate::auth::viewer::Viewer;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Hidden,
    Busy,
    Summary,
    Full,
}

impl Level {
    pub const ALL: [Self; 4] = [Self::Hidden, Self::Busy, Self::Summary, Self::Full];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Busy => "busy",
            Self::Summary => "summary",
            Self::Full => "full",
        }
    }
}

impl FromStr for Level {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "hidden" => Ok(Self::Hidden),
            "busy" => Ok(Self::Busy),
            "summary" => Ok(Self::Summary),
            "full" => Ok(Self::Full),
            _ => Err("level must be hidden, busy, summary, or full"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AudiencePolicy {
    pub public_level: Level,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideKind {
    Include,
    Exclude,
}

#[derive(Debug, Clone, Copy)]
pub struct PersonOverride {
    pub person_id: i64,
    pub kind: OverrideKind,
    pub level: Option<Level>,
}

#[derive(Debug, Clone, Copy)]
pub struct CircleGrant {
    pub circle_id: i64,
    pub level: Level,
}

/// The sole visibility grant calculation used by browsing surfaces.
pub fn level_for(
    viewer: &Viewer,
    policy: &AudiencePolicy,
    overrides: &[PersonOverride],
    circle_grants: &[CircleGrant],
    person_circles: &[i64],
) -> Level {
    if matches!(viewer, Viewer::Owner { .. }) {
        return Level::Full;
    }

    let Some(person_id) = viewer.person_id() else {
        return policy.public_level;
    };

    if let Some(person_override) = overrides.iter().find(|row| row.person_id == person_id) {
        return match person_override.kind {
            OverrideKind::Exclude => Level::Hidden,
            OverrideKind::Include => person_override.level.unwrap_or(Level::Hidden),
        };
    }

    let circle_level = circle_grants
        .iter()
        .filter(|grant| person_circles.contains(&grant.circle_id))
        .map(|grant| grant.level)
        .max()
        .unwrap_or(Level::Hidden);
    circle_level.max(policy.public_level)
}

/// Direct capability-link policy. Link tier is a grant only at this chokepoint.
pub fn level_for_direct_hit(
    viewer: &Viewer,
    link_tier: &str,
    policy: &AudiencePolicy,
    overrides: &[PersonOverride],
    circle_grants: &[CircleGrant],
    person_circles: &[i64],
) -> Level {
    if matches!(viewer, Viewer::Owner { .. }) {
        return Level::Full;
    }
    let explicitly_excluded = viewer.person_id().is_some_and(|person_id| {
        overrides
            .iter()
            .any(|row| row.person_id == person_id && row.kind == OverrideKind::Exclude)
    });
    if explicitly_excluded {
        return Level::Hidden;
    }

    let computed = level_for(viewer, policy, overrides, circle_grants, person_circles);
    match link_tier {
        "private" => computed.max(Level::Full),
        "public" => computed.max(Level::Summary),
        _ => computed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(level: Level) -> AudiencePolicy {
        AudiencePolicy {
            public_level: level,
        }
    }
    fn link(person_id: Option<i64>) -> Viewer {
        Viewer::LinkHolder {
            person_id,
            event_id: 9,
        }
    }

    #[test]
    fn every_viewer_class_gets_the_contract_default() {
        for public in Level::ALL {
            assert_eq!(
                level_for(&Viewer::Anonymous, &policy(public), &[], &[], &[]),
                public
            );
            assert_eq!(
                level_for(&link(None), &policy(public), &[], &[], &[]),
                public
            );
            assert_eq!(
                level_for(&link(Some(7)), &policy(public), &[], &[], &[]),
                public
            );
            assert_eq!(
                level_for(
                    &Viewer::Guest {
                        identity_id: 2,
                        person_id: 7
                    },
                    &policy(public),
                    &[],
                    &[],
                    &[]
                ),
                public
            );
            assert_eq!(
                level_for(
                    &Viewer::Owner { identity_id: 1 },
                    &policy(public),
                    &[],
                    &[],
                    &[]
                ),
                Level::Full
            );
        }
    }

    #[test]
    fn every_include_level_and_exclude_override_all_other_grants() {
        let grants = [CircleGrant {
            circle_id: 3,
            level: Level::Full,
        }];
        for included in Level::ALL {
            let rows = [PersonOverride {
                person_id: 7,
                kind: OverrideKind::Include,
                level: Some(included),
            }];
            assert_eq!(
                level_for(&link(Some(7)), &policy(Level::Full), &rows, &grants, &[3]),
                included
            );
        }
        let rows = [PersonOverride {
            person_id: 7,
            kind: OverrideKind::Exclude,
            level: None,
        }];
        assert_eq!(
            level_for(&link(Some(7)), &policy(Level::Full), &rows, &grants, &[3]),
            Level::Hidden
        );
        assert_eq!(
            level_for(
                &Viewer::Owner { identity_id: 1 },
                &policy(Level::Hidden),
                &rows,
                &[],
                &[]
            ),
            Level::Full
        );
    }

    #[test]
    fn matching_circles_take_most_generous_then_max_with_public() {
        let grants = [
            CircleGrant {
                circle_id: 1,
                level: Level::Busy,
            },
            CircleGrant {
                circle_id: 2,
                level: Level::Full,
            },
            CircleGrant {
                circle_id: 3,
                level: Level::Summary,
            },
        ];
        assert_eq!(
            level_for(&link(Some(7)), &policy(Level::Busy), &[], &grants, &[1, 2]),
            Level::Full
        );
        assert_eq!(
            level_for(&link(Some(7)), &policy(Level::Summary), &[], &grants, &[1]),
            Level::Summary
        );
        assert_eq!(
            level_for(&link(Some(7)), &policy(Level::Busy), &[], &grants, &[99]),
            Level::Busy
        );
    }

    #[test]
    fn direct_link_floors_are_tier_specific_and_exclude_still_wins() {
        for public in Level::ALL {
            assert_eq!(
                level_for_direct_hit(&link(None), "private", &policy(public), &[], &[], &[]),
                Level::Full
            );
            assert_eq!(
                level_for_direct_hit(&link(None), "public", &policy(public), &[], &[], &[]),
                public.max(Level::Summary)
            );
        }
        let excluded = [PersonOverride {
            person_id: 7,
            kind: OverrideKind::Exclude,
            level: None,
        }];
        assert_eq!(
            level_for_direct_hit(
                &link(Some(7)),
                "private",
                &policy(Level::Full),
                &excluded,
                &[],
                &[]
            ),
            Level::Hidden
        );
        assert_eq!(
            level_for_direct_hit(
                &Viewer::Owner { identity_id: 1 },
                "public",
                &policy(Level::Hidden),
                &excluded,
                &[],
                &[]
            ),
            Level::Full
        );
    }
}

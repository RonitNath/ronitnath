//! `cargo run -- seed <account-email>` — seeds the July 4th 2026 gathering
//! plus archived, fully-recovered records of the two previous events
//! (Housewarming, B24), then prints the shareable links. Idempotent:
//! skips any event whose slug already exists.
//!
//! Content sources:
//! - July 4: the logistics research in `~/dev/july4` (timeline, transit,
//!   viewing verdict as of Jul 1–2, 2026).
//! - Housewarming + B24: recovered 2026-07-03 from nexus's `/data/archive`
//!   backups — Housewarming from a `socials_bravo` sqlite snapshot
//!   (`2026-03-12/personal-socials_bravo.tar`), B24 from the old Patroni
//!   Postgres cluster's raw data directory
//!   (`2026-03-20-patroni-pg/patroni-g.tar.gz`, booted read-only under
//!   Docker to query the `socials` schema). Entry-instruction photos
//!   recovered from gateway's `/var/lib/socials/images` live in
//!   `static/img/entry-1-pine/`.

use crate::auth::session::{generate_token, hash_token};
use crate::store::Store;
use crate::store::events::EventFields;
use crate::store::schedule_items::ScheduleItemFields;

pub async fn run(store: &Store, email: &str, public_url: &str) -> anyhow::Result<()> {
    let email = email.trim().to_lowercase();
    let factor = store
        .find_factor_by_external("password", &email)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!("no identity with email {email} — sign up at /signup first")
        })?;
    let membership = store
        .find_primary_membership(factor.identity_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("identity has no account membership"))?;
    let account_id = membership.account_id;

    seed_housewarming(store, account_id).await?;
    seed_b24(store, account_id).await?;
    seed_july4(store, account_id, public_url).await?;
    set_headcounts(store, account_id).await?;
    Ok(())
}

/// Landing-page attendee numbers. Display-only and set by hand (see the
/// `headcount` column comment) — applied even when the events themselves
/// already exist, so re-running seed refreshes them.
async fn set_headcounts(store: &Store, account_id: i64) -> anyhow::Result<()> {
    for (slug, headcount) in [("july4-2026", 23), ("b24", 44), ("housewarming-2025", 18)] {
        if let Some(event) = store.find_event_by_slug(account_id, slug).await? {
            store
                .set_event_headcount(account_id, event.id, Some(headcount))
                .await?;
        }
    }
    Ok(())
}

/// Imports a flat guest list as `people` + `attendance` rows, matching an
/// existing person by case-insensitive name (so re-running seed, or a
/// later event's bulk-add, accumulates onto the same person rather than
/// duplicating them — the whole point of the longitudinal `people` table).
async fn import_guests(
    store: &Store,
    account_id: i64,
    event_id: i64,
    guests: &[(&str, &str, i64, &str)],
) -> anyhow::Result<()> {
    let existing = store.list_people(account_id).await?;
    for (name, status, party_size, note) in guests {
        let person_id = match existing.iter().find(|p| p.name.eq_ignore_ascii_case(name)) {
            Some(p) => p.id,
            None => store.create_person(account_id, name, "").await?.id,
        };
        store
            .upsert_attendance(account_id, event_id, person_id, status, *party_size, note)
            .await?;
    }
    Ok(())
}

/// (name, status, party_size, note) — recovered from `socials_bravo`'s
/// `db.sqlite` (`parties`/`party_members` tables), Dec 2025 snapshot.
/// `status` is `attended` (RSVP'd yes), `no`, `maybe`, or `none`
/// (never responded). `party_size` and `note` capture the rest of that
/// person's party where the old schema tracked one further than a single
/// name (e.g. "Joe" brought "Jaysa, Sophie, one_more").
const HOUSEWARMING_GUESTS: &[(&str, &str, i64, &str)] = &[
    ("Monisha", "attended", 1, ""),
    ("Minh", "attended", 1, ""),
    ("Rosie", "attended", 1, ""),
    ("Joie", "attended", 1, ""),
    ("Joe", "no", 4, "with Jaysa, Sophie, one_more"),
    ("Finna", "attended", 1, ""),
    ("Fang", "attended", 2, ""),
    ("Julia", "attended", 1, ""),
    ("Sharon", "attended", 1, ""),
    ("Nora", "attended", 2, "with bf"),
    ("Lara", "no", 1, ""),
    ("Luke", "none", 1, ""),
    ("Knives", "no", 1, ""),
    ("Mina", "none", 1, ""),
    ("Jingwen", "no", 67, ""), // yes, a real RSVP of "no" for a party of 67
    ("Celina", "no", 1, ""),
    ("Dan", "no", 1, ""),
    ("Ruben", "none", 1, ""),
    ("Alex", "none", 1, ""),
    ("rjz", "none", 1, ""),
    ("Oliver", "none", 1, ""),
    ("Sawan", "no", 1, ""),
    ("Chenghao", "no", 1, ""),
    ("Aditya", "no", 1, ""),
    ("Nick", "attended", 1, ""),
    ("Yug", "none", 1, ""),
    ("Leo", "attended", 1, ""),
    ("Alex", "attended", 2, "with Michael"),
    ("Kevin", "no", 1, ""),
    ("Jo", "none", 1, ""),
    ("Tiffany", "none", 1, "with bf"),
    ("Swathy", "no", 1, ""),
    ("Fatima", "no", 1, ""),
    ("Bethany", "none", 1, ""),
    ("Caden", "attended", 2, ""),
    ("Abdul", "none", 1, ""),
    ("Alara", "none", 1, ""),
    ("Nikhil", "attended", 1, ""),
    ("Sid", "none", 2, "with Sofia"),
];

async fn seed_housewarming(store: &Store, account_id: i64) -> anyhow::Result<()> {
    if store
        .find_event_by_slug(account_id, "housewarming-2025")
        .await?
        .is_some()
    {
        println!("housewarming-2025 already exists — leaving it alone");
        return Ok(());
    }

    let event = store
        .create_event(
            account_id,
            "housewarming-2025",
            "Housewarming",
            "2025-12-06 17:00",
        )
        .await?;
    let (directions, entry_instructions) = directions_and_entry_html_housewarming();
    let fields = EventFields {
        slug: "housewarming-2025".into(),
        title: "Housewarming".into(),
        tagline: "The first gathering at the new place".into(),
        starts_at: "2025-12-06 17:00".into(),
        ends_at: Some("2025-12-06 20:00".into()),
        timezone: "America/Los_Angeles".into(),
        status: "archived".into(),
        summary: "I've just moved into my new apartment in San Francisco. Come over and \
                  spend some time at my new place; we'll have pizza, non-alcoholic drinks, \
                  pastries, video games, board games, music, and lots of friends! We'll \
                  likely go 5-8pm. You're welcome to invite anyone over, just make sure to \
                  update your group size in your RSVP."
            .into(),
        area_name: "FiDi, San Francisco".into(),
        address: "1 Pine Street, San Francisco".into(),
        entry_instructions,
        private_details: directions,
        notice_html: String::new(),
        quick_plan_html: String::new(),
    };
    store.update_event(account_id, event.id, &fields).await?;

    import_guests(store, account_id, event.id, HOUSEWARMING_GUESTS).await?;
    println!(
        "seeded: Housewarming (archived) — {} recovered guests",
        HOUSEWARMING_GUESTS.len()
    );
    Ok(())
}

fn directions_and_entry_html_housewarming() -> (String, String) {
    let directions = "If you're taking transit, you can find the building right above the \
        Embarcadero BART Station. If you're driving, good luck parking! If you're \
        ridesharing, set your drop-off as 1 Pine. Here's what it looks like:\n\
        <img src=\"/static/img/entry-1-pine/which-building.png\" alt=\"Which building — 1 Pine vs 338 Market\" />\n\
        If you're taking BART, take exit A3 at the Embarcadero station:\n\
        <img src=\"/static/img/entry-1-pine/bart-exit.jpg\" alt=\"BART exit 3A, underground\" />\n\
        Then turn directly around to go towards the ferry building, and round the corner of \
        the building to find the entrance.\n\
        <img src=\"/static/img/entry-1-pine/bart-exit-street.jpg\" alt=\"Street view after the BART exit\" />"
        .to_string();
    let entry = "First, make sure you're entering 1 Pine, and not 338 Market. Here's what the \
        building's residential entrance looks like:\n\
        <img src=\"/static/img/entry-1-pine/entrance.jpg\" alt=\"1 Pine residential entrance\" />\n\
        Wave to the desk inside to let them know to unlock the door. Tell them you're here \
        for Ronit's housewarming in 2210. They may ask you to sign in, and/or send a call up. \
        After this, they should escort you to the elevator and badge you in. Take the \
        elevator to the 22nd floor. Once out, look for 2210 (halfway down the hall on the \
        right)."
        .to_string();
    (directions, entry)
}

/// (name, status, party_size, note) — recovered from the old Patroni
/// Postgres cluster's `socials` schema, March 2026 snapshot. `party_size`
/// folds in that person's `plus_ones` (the old schema tracked companions
/// as loose names, not full guest records).
const B24_GUESTS: &[(&str, &str, i64, &str)] = &[
    ("Adi", "attended", 1, ""),
    ("Alex McDowell", "attended", 1, ""),
    ("Asai", "attended", 1, ""),
    ("Bethany", "attended", 1, ""),
    ("Brooke", "attended", 1, ""),
    ("Caden", "attended", 2, "with DeAgo"),
    ("C E L E N E", "attended", 1, ""),
    ("Chenghao", "attended", 1, ""),
    ("DANLIU", "attended", 1, ""),
    ("xx_starcraftEnjoyer_xx", "attended", 2, "with Lydia Wang"),
    ("Finna Cullen", "attended", 1, ""),
    ("Isabelle", "attended", 1, ""),
    ("Ishaan", "attended", 1, ""),
    (
        "users.users.jaysa",
        "attended",
        2,
        "with hunter (from turlock)",
    ),
    ("Jingwen", "attended", 1, ""),
    ("Joe WANG", "attended", 1, ""),
    ("Jojoie", "maybe", 2, "with Han Wu (maybe)"),
    ("Kevin", "attended", 1, ""),
    ("Kian", "attended", 1, ""),
    ("Kinn", "attended", 3, "with Michael, Windy"),
    ("Knives", "attended", 1, ""),
    ("Vtuber", "attended", 1, ""),
    ("Leo", "none", 1, ""),
    ("Luke", "attended", 1, ""),
    ("Mannan", "attended", 1, ""),
    ("turtlebasket", "attended", 1, ""),
    ("Mina", "attended", 1, ""),
    ("tiraMinhsu", "attended", 1, ""),
    ("Nick", "attended", 1, ""),
    ("Nick Castello", "attended", 1, ""),
    ("Nikhil", "attended", 1, ""),
    ("Nora", "no", 2, "with Arthur"),
    ("Php", "attended", 1, ""),
    ("Precious", "attended", 2, "with Precious's BF"),
    ("Furniture", "attended", 1, ""),
    ("Otome Tohoten", "attended", 1, ""),
    ("Ruben", "no", 1, ""),
    ("Sawan", "attended", 1, ""),
    ("Sharon", "attended", 1, ""),
    ("Sophie", "attended", 1, ""),
    ("Steven", "attended", 2, "with Amy"),
    ("Tara", "attended", 1, ""),
    ("Victoria", "attended", 1, ""),
    ("Yena", "maybe", 1, ""),
    ("Yug", "no", 1, ""),
    ("Zach", "attended", 1, ""),
];

async fn seed_b24(store: &Store, account_id: i64) -> anyhow::Result<()> {
    if store.find_event_by_slug(account_id, "b24").await?.is_some() {
        println!("b24 already exists — leaving it alone");
        return Ok(());
    }

    let event = store
        .create_event(
            account_id,
            "b24",
            "Ronit's 24th - Rooftop Party",
            "2026-03-15 16:00",
        )
        .await?;
    let (directions, entry) = directions_and_entry_html_b24();
    let fields = EventFields {
        slug: "b24".into(),
        title: "Ronit's 24th - Rooftop Party".into(),
        tagline: "Theme: Starlight".into(),
        starts_at: "2026-03-15 16:00".into(),
        ends_at: None,
        timezone: "America/Los_Angeles".into(),
        status: "archived".into(),
        summary: "I cordially invite you to come celebrate my birthday and hope you have an \
                  opportunity to meet some of the other people whom I've had the opportunity \
                  and pleasure to count among my friends (and family). We will have pastries \
                  and drinks (alcoholic and nonalcoholic) around the clock, dinner from a \
                  restaurant nearby delivered, and a cake cutting ceremony. Additionally, we \
                  may have an early group picture for some of the people who may need to \
                  leave early."
            .into(),
        area_name: "FiDi, San Francisco".into(),
        address: "1 Pine Street, San Francisco".into(),
        entry_instructions: entry,
        private_details: directions,
        notice_html: String::new(),
        quick_plan_html: String::new(),
    };
    store.update_event(account_id, event.id, &fields).await?;

    for (order, (time, title)) in [
        ("4:00 PM", "Arrive / hang out"),
        ("~Sunset", "Early group photo"),
        ("6:00 PM", "Dinner delivered"),
        ("7:30 PM", "Cake cutting"),
        ("8:00 PM", "Rooftop hang continues"),
    ]
    .iter()
    .enumerate()
    {
        store
            .create_schedule_item(
                account_id,
                event.id,
                &ScheduleItemFields {
                    sort_order: order as i64,
                    time_label: (*time).into(),
                    title: (*title).into(),
                    detail: String::new(),
                    tier: "public".into(),
                    segment_key: None,
                },
            )
            .await?;
    }

    import_guests(store, account_id, event.id, B24_GUESTS).await?;
    println!(
        "seeded: B24 (archived) — {} recovered guests",
        B24_GUESTS.len()
    );
    Ok(())
}

fn directions_and_entry_html_b24() -> (String, String) {
    let directions = "If you're taking transit, you can find the building right above the \
        Embarcadero BART Station. If you're driving, good luck parking! If you're \
        ridesharing, set your drop-off as 1 Pine. Here's what it looks like:\n\
        <img src=\"/static/img/entry-1-pine/which-building.png\" alt=\"Which building — 1 Pine vs 338 Market\" />\n\
        If you're taking BART, take exit 3A at the Embarcadero station:\n\
        <img src=\"/static/img/entry-1-pine/bart-exit.jpg\" alt=\"BART exit 3A, underground\" />\n\
        Then turn directly around to go towards the ferry building, and round the corner of \
        the building to find the entrance.\n\
        <img src=\"/static/img/entry-1-pine/bart-exit-street.jpg\" alt=\"Street view after the BART exit\" />"
        .to_string();
    let entry = "Make sure you're entering <strong>1 Pine</strong>, and not 338 Market. \
        Here's what the residential entrance looks like:\n\
        <img src=\"/static/img/entry-1-pine/entrance.jpg\" alt=\"1 Pine residential entrance\" />\n\
        Wave to the desk inside to let them know to unlock the door. Tell them you're here \
        for <strong>Ronit's birthday on the rooftop</strong>. They may ask you to sign in. \
        After this, take the lobby elevator directly to the <strong>26th floor</strong>. Once \
        there, turn directly left or right to the third elevator and take it up to the \
        <strong>rooftop</strong>.\n\
        <strong>Restroom:</strong> head down to apartment <strong>2210</strong> on the 22nd \
        floor (halfway down the hall on the right)."
        .to_string();
    (directions, entry)
}

async fn seed_july4(store: &Store, account_id: i64, public_url: &str) -> anyhow::Result<()> {
    if store
        .find_event_by_slug(account_id, "july4-2026")
        .await?
        .is_some()
    {
        println!("july4-2026 already exists — leaving it alone");
        return Ok(());
    }

    let event = store
        .create_event(
            account_id,
            "july4-2026",
            "July 4th Party",
            "2026-07-04 13:00",
        )
        .await?;
    let (directions, _) = directions_and_entry_html_b24();
    let fields = EventFields {
        slug: "july4-2026".into(),
        title: "July 4th Party".into(),
        tagline: "Board games → dinner → Golden Gate Bridge Fireworks → rooftop midnight fireworks → some sleepover".into(),
        starts_at: "2026-07-04 13:00".into(),
        ends_at: Some("2026-07-05 10:00".into()),
        timezone: "America/Los_Angeles".into(),
        status: "published".into(),
        summary: "The city's launching fireworks off GGB for only the third time ever for \
                  America's 250th birthday party! We're starting the day by meeting at my \
                  apartment, chatting, and playing some board games, and then going to \
                  Harborview (2 min walk) for dinner at 5:30 PM. Then, we're going to travel \
                  over to Marina Green afterwards, ideally securing some space by 7:30pm to \
                  see the fireworks off the bridge. We may redirect to Fort Mason depending on \
                  how busy things get. Afterwards, some people will be heading home, but we'll \
                  have an afterparty at my place, including going up to my rooftop to watch any \
                  of the fireworks happening at midnight over the Bay Bridge area (we have a \
                  direct view through to there and the Ferry Building and Berkeley). The last \
                  train leaves at 12:10am, and some people are travelling in from e.g. South \
                  Bay, so a bunch of people will be sleeping over as well. If this is you, \
                  please bring a sleeping bag. I'm looking forward to seeing you all here!!!"
            .into(),
        area_name: "FiDi, San Francisco (exact address on your personal invite)".into(),
        address: "1 Pine Street, San Francisco".into(),
        // Same building as Housewarming and B24 — reuses the real,
        // recovered entry instructions/photos (front desk sign-in, badge
        // up to 22).
        entry_instructions: "Enter through the 1 Pine St. residential entrance.\n\
            <img src=\"/static/img/entry-1-pine/entrance.jpg\" alt=\"1 Pine residential entrance\" />\n\
            Wave to the desk inside to let them know to unlock the door. Tell them you're here \
            for Ronit's July 4th party in 2210. They may ask you to sign in and/or send a call \
            up. Take the elevator to the 22nd floor; 2210 is halfway down the hall on the right."
            .into(),
        private_details: format!(
            "{directions}\n\n\
             Last trains out are ~12:21 AM (Millbrae) / ~12:31 \
             AM (Fremont); Berkeley crew has the 800 All-Nighter until 3:30 AM."
        ),
        notice_html: "<p><strong>Ensure to inform Ronit of your arrival/join time, and \
            whether you're making it to dinner!</strong> Contact at \
            <a href=\"sms:+19096952320\">+1 (909) 695-2320</a>. Event is open invite.</p>\n\
            <p>It gets cold after dark down by the water — bring a jacket.</p>\n\
            <p>People from afar are sleeping over; please bring a sleeping bag.</p>"
            .into(),
        quick_plan_html: "<ul>\n\
            <li>Board games start at 2pm</li>\n\
            <li>Dinner at <a href=\"https://www.harborviewsf.com/\">Harborview</a> at 5:30pm</li>\n\
            <li>Firework watching from Marina Green at 7pm</li>\n\
            <li>Midnight fireworks from Ronit's place</li>\n\
            </ul>"
            .into(),
    };
    store.update_event(account_id, event.id, &fields).await?;

    // (sort, time, title, detail, tier, segment_key)
    let schedule: &[(i64, &str, &str, &str, &str, Option<&str>)] = &[
        (
            0,
            "2:00 PM",
            "Doors open — board games & hang",
            "Roll in whenever.",
            "public",
            Some("board_games"),
        ),
        (
            2,
            "5:30 PM",
            "Dinner — Harborview",
            "<a href=\"https://www.harborviewsf.com/\">Harborview</a> in Embarcadero Center. \
          $25–35 via Zelle to 9096952320.",
            "public",
            Some("dinner"),
        ),
        (
            3,
            "7:00ish PM",
            "Bus to the Marina",
            "The <a href=\"https://www.sfmta.com/routes/30-stockton\">30 Stockton</a> runs from near \
          the dinner spot to the Marina, and Muni is adding \
          <a href=\"https://www.sfmta.com/project-updates/july-4th-extra-service#5pm\">extra July 4th \
          service from 5 PM</a> (plus the fireworks shuttle from Powell & Embarcadero BART to Marina \
          Middle School, 4:00–11:30 PM).",
            "public",
            None,
        ),
        (
            4,
            "7:30 PM",
            "Post up Marina Green",
            "Nearly head-on view of the bridge.",
            "public",
            Some("fireworks"),
        ),
        (
            5,
            "9:30 PM",
            "Fireworks off the Golden Gate Bridge",
            "The 250th-anniversary show, launched from both bridge towers + bay barges. ~25 minutes.",
            "public",
            None,
        ),
        (
            7,
            "11:30 PM",
            "Rooftop afterparty",
            "",
            "public",
            Some("rooftop"),
        ),
        (
            9,
            "Overnight",
            "Sleepover",
            "Couches + floor space + blankets. First trains home ~8 AM Sunday; lazy breakfast before that.",
            "public",
            Some("sleepover"),
        ),
    ];
    for (sort, time, title, detail, tier, segment) in schedule {
        store
            .create_schedule_item(
                account_id,
                event.id,
                &ScheduleItemFields {
                    sort_order: *sort,
                    time_label: (*time).into(),
                    title: (*title).into(),
                    detail: (*detail).into(),
                    tier: (*tier).into(),
                    segment_key: segment.map(Into::into),
                },
            )
            .await?;
    }

    // Two shareable links to start with; personalized ones get minted from
    // the admin page (or automatically on public-link self-signups).
    for (label, tier) in [("open share", "public"), ("trusted share", "private")] {
        let raw = generate_token();
        store
            .create_event_link(
                account_id,
                event.id,
                None,
                &hash_token(&raw),
                &raw,
                label,
                tier,
            )
            .await?;
        println!("{label} ({tier}): {public_url}/e/{raw}");
    }

    println!(
        "seeded: July 4th Party (published) — manage at {public_url}/events/{}",
        event.id
    );
    Ok(())
}

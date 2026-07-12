use std::io::{self, BufRead};

use anyhow::{Context, bail};
use ronitnath::access::level::Level;
use ronitnath::auth::session::hash_token;
use ronitnath::config::Config;
use ronitnath::store::Store;
use ronitnath::store::event_links::personal_token;
use ronitnath::store::events::InviteField;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    ronitnath::telemetry::init();

    if let Err(err) = dispatch().await {
        eprintln!("admin failed: {err:#}");
        std::process::exit(1);
    }
}

async fn dispatch() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("seed") => {
            let email = args.get(2).context("usage: admin seed <account-email>")?;
            let config = Config::from_env();
            let store = Store::connect_existing(&config.database_url).await?;
            ronitnath::seed::run(&store, email, &config.public_url).await
        }
        Some("mint-link") => mint_link(&args).await,
        Some("list-links") => list_links(&args).await,
        Some("revoke-link") => revoke_link(&args).await,
        Some("add-people") => add_people(&args).await,
        Some("set-status") => set_status(&args).await,
        Some("set-segment") => set_segment(&args).await,
        Some("set-invite") => set_invite(&args).await,
        Some("set-headcount") => set_headcount(&args).await,
        Some("set-audience") => set_audience(&args).await,
        Some(flag) if flag == "-h" || flag == "--help" => {
            print_usage(&args[0]);
            Ok(())
        }
        Some(other) => bail!("unknown subcommand {other:?}"),
        None => {
            ronitnath::app::run_admin().await;
            Ok(())
        }
    }
}

fn print_usage(bin: &str) {
    eprintln!("usage:");
    eprintln!("  {bin}                         # start admin server");
    eprintln!("  {bin} seed <email>");
    eprintln!(
        "  {bin} mint-link <event-slug> --person \"<name>\" --tier <public|private> --label <label>"
    );
    eprintln!(
        "      # personalized token \"name-xxxx\"; revokes + replaces the person's live link"
    );
    eprintln!("  {bin} list-links <event-slug>               # every live link with its URL");
    eprintln!("  {bin} revoke-link <event-slug> --person \"<name>\"");
    eprintln!(
        "  {bin} add-people <event-slug> [--status <status>]   # reads Name | group from stdin"
    );
    eprintln!("  {bin} set-status <event-slug> \"<name>\" <status>");
    eprintln!(
        "  {bin} set-segment <event-slug> <segment-key> \"<name>\" [--status <in|maybe|out>] [--paid <yes|no>] [--attended <yes|no>]"
    );
    eprintln!("  {bin} set-invite <event-slug> <notice|quick-plan>   # reads HTML from stdin");
    eprintln!("  {bin} set-headcount <event-slug> <n|none>   # landing-page attendee number");
    eprintln!("  {bin} set-audience <event-slug> --public <level>");
    eprintln!("  {bin} set-audience <event-slug> --circle \"<name>=<level>\"");
    eprintln!("  {bin} set-audience <event-slug> --person \"<name>=<include:level|exclude>\"");
}

async fn open_cli_store() -> anyhow::Result<(Config, Store, i64)> {
    let config = Config::from_env();
    let store = Store::connect_existing(&config.database_url).await?;
    let account_id = store.require_primary_account().await?;
    Ok((config, store, account_id))
}

async fn event_id_for_slug(store: &Store, account_id: i64, slug: &str) -> anyhow::Result<i64> {
    Ok(store
        .find_event_by_slug(account_id, slug)
        .await?
        .with_context(|| format!("no event with slug {slug:?}"))?
        .id)
}

async fn person_id_for_name(
    store: &Store,
    account_id: i64,
    name: &str,
    group: &str,
) -> anyhow::Result<i64> {
    let name = name.trim();
    if name.is_empty() {
        bail!("person name is required");
    }
    if let Some(person) = store.find_person_by_name(account_id, name).await? {
        return Ok(person.id);
    }
    Ok(store
        .create_person(account_id, name, group.trim())
        .await?
        .id)
}

async fn mint_link(args: &[String]) -> anyhow::Result<()> {
    let slug = args.get(2).context(
        "usage: admin mint-link <event-slug> --person \"<name>\" --tier <public|private> --label <label>",
    )?;
    let person = required_flag(args, "--person")?;
    let tier = required_flag(args, "--tier")?;
    let label = required_flag(args, "--label")?;
    validate_tier(tier)?;

    let (config, store, account_id) = open_cli_store().await?;
    let event_id = event_id_for_slug(&store, account_id, slug).await?;
    let person_id = person_id_for_name(&store, account_id, person, "").await?;

    // One live personalized link per person per event: re-minting is the
    // invalidation story (same name, fresh suffix), so retire the old one.
    if let Some(existing) = store
        .find_personal_link(account_id, event_id, person_id)
        .await?
    {
        store.revoke_event_link(account_id, existing.id).await?;
        eprintln!("revoked previous link /e/{}", existing.token_plain);
    }

    let raw = personal_token(person);
    store
        .create_event_link(
            account_id,
            event_id,
            Some(person_id),
            &hash_token(&raw),
            &raw,
            label.trim(),
            tier,
        )
        .await?;
    println!("{}/e/{raw}", config.public_url);
    Ok(())
}

async fn list_links(args: &[String]) -> anyhow::Result<()> {
    let slug = args
        .get(2)
        .context("usage: admin list-links <event-slug>")?;
    let (config, store, account_id) = open_cli_store().await?;
    let event_id = event_id_for_slug(&store, account_id, slug).await?;
    for link in store.list_event_links(account_id, event_id).await? {
        if link.revoked_at.is_some() {
            continue;
        }
        let who = link.person_name.as_deref().unwrap_or("(shared)");
        println!(
            "{who}\t{}\t{}\tuses={}\t{}/e/{}",
            link.label, link.tier, link.uses, config.public_url, link.token_plain
        );
    }
    Ok(())
}

async fn revoke_link(args: &[String]) -> anyhow::Result<()> {
    let slug = args
        .get(2)
        .context("usage: admin revoke-link <event-slug> --person \"<name>\"")?;
    let person = required_flag(args, "--person")?;

    let (_config, store, account_id) = open_cli_store().await?;
    let event_id = event_id_for_slug(&store, account_id, slug).await?;
    let person_id = person_id_for_name(&store, account_id, person, "").await?;
    let link = store
        .find_personal_link(account_id, event_id, person_id)
        .await?
        .with_context(|| format!("no live personalized link for {person:?}"))?;
    store.revoke_event_link(account_id, link.id).await?;
    println!("revoked /e/{}", link.token_plain);
    Ok(())
}

async fn set_segment(args: &[String]) -> anyhow::Result<()> {
    let usage = "usage: admin set-segment <event-slug> <segment-key> \"<name>\" \
                 [--status <in|maybe|out>] [--paid <yes|no>] [--attended <yes|no>]";
    let slug = args.get(2).context(usage)?;
    let segment_key = args.get(3).context(usage)?;
    let name = args.get(4).context(usage)?;
    let status = optional_flag(args, "--status");
    if let Some(status) = status {
        if !matches!(status, "in" | "maybe" | "out") {
            bail!("--status must be in, maybe, or out");
        }
    }
    let paid = optional_flag(args, "--paid")
        .map(parse_yes_no)
        .transpose()?;
    let attended = optional_flag(args, "--attended")
        .map(parse_yes_no)
        .transpose()?;
    if status.is_none() && paid.is_none() && attended.is_none() {
        bail!("nothing to set — pass at least one of --status/--paid/--attended");
    }

    let (_config, store, account_id) = open_cli_store().await?;
    let event_id = event_id_for_slug(&store, account_id, slug).await?;
    let person_id = person_id_for_name(&store, account_id, name, "").await?;
    let updated = store
        .set_segment_flags(
            account_id,
            event_id,
            segment_key,
            person_id,
            status,
            paid,
            attended,
        )
        .await?;
    if updated == 0 {
        bail!("no segment {segment_key:?} on event {slug:?}");
    }
    Ok(())
}

async fn set_invite(args: &[String]) -> anyhow::Result<()> {
    let usage =
        "usage: admin set-invite <event-slug> <notice|quick-plan>   # reads HTML from stdin";
    let slug = args.get(2).context(usage)?;
    let field = match args.get(3).map(String::as_str) {
        Some("notice") => InviteField::Notice,
        Some("quick-plan") => InviteField::QuickPlan,
        _ => bail!("{usage}"),
    };
    let html = io::read_to_string(io::stdin())?;

    let (_config, store, account_id) = open_cli_store().await?;
    let event_id = event_id_for_slug(&store, account_id, slug).await?;
    store
        .set_invite_content(account_id, event_id, field, html.trim())
        .await?;
    Ok(())
}

fn parse_yes_no(value: &str) -> anyhow::Result<bool> {
    match value {
        "yes" | "true" | "1" => Ok(true),
        "no" | "false" | "0" => Ok(false),
        other => bail!("expected yes or no, got {other:?}"),
    }
}

async fn add_people(args: &[String]) -> anyhow::Result<()> {
    let slug = args
        .get(2)
        .context("usage: admin add-people <event-slug> [--status <status>]")?;
    let status = optional_flag(args, "--status").unwrap_or("none");
    validate_status(status)?;

    let (_config, store, account_id) = open_cli_store().await?;
    let event_id = event_id_for_slug(&store, account_id, slug).await?;
    for line in io::stdin().lock().lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (name, group) = line
            .split_once('|')
            .map_or((line, ""), |(name, group)| (name.trim(), group.trim()));
        let person_id = person_id_for_name(&store, account_id, name, group).await?;
        store
            .upsert_attendance(account_id, event_id, person_id, status, 1, "")
            .await?;
    }
    Ok(())
}

async fn set_status(args: &[String]) -> anyhow::Result<()> {
    let slug = args
        .get(2)
        .context("usage: admin set-status <event-slug> \"<name>\" <status>")?;
    let name = args
        .get(3)
        .context("usage: admin set-status <event-slug> \"<name>\" <status>")?;
    let status = args
        .get(4)
        .context("usage: admin set-status <event-slug> \"<name>\" <status>")?;
    validate_status(status)?;

    let (_config, store, account_id) = open_cli_store().await?;
    let event_id = event_id_for_slug(&store, account_id, slug).await?;
    let person_id = person_id_for_name(&store, account_id, name, "").await?;
    let existing = store
        .find_attendance(account_id, event_id, person_id)
        .await?;
    let (party_size, note) = existing.map_or((1, String::new()), |attendance| {
        (attendance.party_size, attendance.note)
    });
    store
        .upsert_attendance(account_id, event_id, person_id, status, party_size, &note)
        .await?;
    Ok(())
}

async fn set_headcount(args: &[String]) -> anyhow::Result<()> {
    let usage = "usage: admin set-headcount <event-slug> <n|none>";
    let slug = args.get(2).context(usage)?;
    let value = args.get(3).context(usage)?;
    let headcount = match value.as_str() {
        "none" => None,
        n => Some(n.parse::<i64>().context(usage)?),
    };

    let (_config, store, account_id) = open_cli_store().await?;
    let event_id = event_id_for_slug(&store, account_id, slug).await?;
    store
        .set_event_headcount(account_id, event_id, headcount)
        .await?;
    Ok(())
}

async fn set_audience(args: &[String]) -> anyhow::Result<()> {
    let usage = "usage: admin set-audience <event-slug> (--public <level> | --circle <name>=<level> | --person <name>=<include:level|exclude>)";
    let slug = args.get(2).context(usage)?;
    let public = optional_flag(args, "--public");
    let circle = optional_flag(args, "--circle");
    let person = optional_flag(args, "--person");
    if [public.is_some(), circle.is_some(), person.is_some()]
        .into_iter()
        .filter(|set| *set)
        .count()
        != 1
    {
        bail!("{usage}");
    }

    let (_config, store, account_id) = open_cli_store().await?;
    let event_id = event_id_for_slug(&store, account_id, slug).await?;
    let policy = store
        .find_audience_policy(account_id, "event", event_id)
        .await?
        .context("event has no audience policy")?;

    if let Some(level) = public {
        level.parse::<Level>().map_err(anyhow::Error::msg)?;
        store.set_public_level(account_id, policy.id, level).await?;
        println!("{slug}: public={level}");
    } else if let Some(spec) = circle {
        let (name, level) = spec.split_once('=').context(usage)?;
        level.parse::<Level>().map_err(anyhow::Error::msg)?;
        let circle = store
            .find_circle_by_name(account_id, name.trim())
            .await?
            .with_context(|| format!("no circle named {:?}", name.trim()))?;
        store
            .set_circle_grant(account_id, policy.id, circle.id, Some(level))
            .await?;
        println!("{slug}: circle {:?}={level}", circle.name);
    } else if let Some(spec) = person {
        let (name, value) = spec.split_once('=').context(usage)?;
        let person = store
            .find_person_by_name(account_id, name.trim())
            .await?
            .with_context(|| format!("no person named {:?}", name.trim()))?;
        let (kind, level) = if value == "exclude" {
            ("exclude", None)
        } else if let Some(level) = value.strip_prefix("include:") {
            level.parse::<Level>().map_err(anyhow::Error::msg)?;
            ("include", Some(level))
        } else {
            bail!("person value must be include:<level> or exclude");
        };
        store
            .set_person_override(account_id, policy.id, person.id, Some(kind), level)
            .await?;
        println!("{slug}: person {:?}={value}", person.name);
    }
    store
        .audit(
            None,
            Some(account_id),
            None,
            "audience.updated",
            "event",
            Some(&event_id.to_string()),
            &serde_json::json!({"source": "cli"}),
        )
        .await?;
    Ok(())
}

fn required_flag<'a>(args: &'a [String], flag: &str) -> anyhow::Result<&'a str> {
    optional_flag(args, flag).with_context(|| format!("missing {flag}"))
}

fn optional_flag<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].as_str())
}

fn validate_tier(tier: &str) -> anyhow::Result<()> {
    if matches!(tier, "public" | "private") {
        Ok(())
    } else {
        bail!("tier must be public or private")
    }
}

fn validate_status(status: &str) -> anyhow::Result<()> {
    if matches!(status, "none" | "going" | "maybe" | "no" | "attended") {
        Ok(())
    } else {
        bail!("status must be none, going, maybe, no, or attended")
    }
}

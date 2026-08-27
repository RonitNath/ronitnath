//! Fill a running cluster with the world the profiles measure, through the
//! real API and nothing else.
//!
//! No fixture writes a row here. Every person is a `POST /auth/register`, every
//! document a `POST /api/cmd/create-document`, every grant a
//! `POST /api/cmd/share` — so the data the load runs against went through the
//! same commands, the same raft commits and the same id derivation a browser
//! would have driven, and a number measured on it is a number about the
//! product rather than about a seeder's shortcut.
//!
//! What it leaves behind is `seed.json`: the session tokens, the owner's
//! person id, the document the fan-out probe edits, and the documents the
//! commit profile writes to — one per worker, because `edit-document` guards
//! on `expected_rev` and two workers on one row would be measuring the
//! lost-update refusal instead of the commit path.

use std::time::Instant;

use serde_json::{Value, json};

use crate::http::Conn;
use crate::key;

/// A registered caller: the cookie it holds, and the person it acts as.
pub struct Caller {
    pub token: String,
    pub person: String,
}

/// Register one identity and read back the person it created.
pub async fn register(conn: &mut Conn, display: &str, email: &str) -> Result<Caller, String> {
    let form = format!(
        "display_name={}&email={}&password={}",
        urlencode(display),
        urlencode(email),
        urlencode("perf-harness-password-1")
    );
    let reply = conn
        .post(
            "/auth/register",
            None,
            ("application/x-www-form-urlencoded", &form),
        )
        .await
        .map_err(|error| error.to_string())?;
    if reply.status != 303 {
        return Err(format!("register answered {} — {}", reply.status, reply.body));
    }
    let token = reply
        .session()
        .ok_or_else(|| "register minted no session cookie".to_owned())?;

    let whoami = conn
        .get("/api/whoami", Some(&token))
        .await
        .map_err(|error| error.to_string())?;
    let person = whoami.json()?["person"]["public_id"]
        .as_str()
        .ok_or_else(|| format!("whoami named no person: {}", whoami.body))?
        .to_owned();
    Ok(Caller { token, person })
}

/// Run a command and hand back its `result`, refusing anything but a 200.
pub async fn command(
    conn: &mut Conn,
    token: &str,
    name: &str,
    args: Value,
) -> Result<Value, String> {
    let mut body = args.as_object().cloned().unwrap_or_default();
    body.insert("key".to_owned(), json!(key::uuid()));
    let payload = Value::Object(body).to_string();
    let reply = conn
        .post(
            &format!("/api/cmd/{name}"),
            Some(token),
            ("application/json", &payload),
        )
        .await
        .map_err(|error| error.to_string())?;
    if reply.status != 200 {
        return Err(format!("{name} answered {} — {}", reply.status, reply.body));
    }
    Ok(reply.json()?["result"].clone())
}

/// Everything the profiles need, written once and read by every other
/// subcommand.
pub struct Plan {
    pub host: String,
    pub documents: usize,
    pub subscribers: usize,
    pub workers: usize,
    pub out: String,
}

pub async fn run(plan: Plan) -> Result<(), String> {
    let stamp = key::stamp();
    let started = Instant::now();
    let mut conn = Conn::connect(&plan.host)
        .await
        .map_err(|error| format!("connect {}: {error}", plan.host))?;

    let owner = register(
        &mut conn,
        "Perf Owner",
        &format!("perf-owner-{stamp}@perf.invalid"),
    )
    .await?;
    eprintln!("seed: owner registered as {}", owner.person);

    // The bulk. Eight connections rather than one, because seeding is not a
    // measurement and a serial 10⁴ raft commits is most of an hour.
    let bulk = fanned(&plan, &owner.token, &stamp, plan.documents).await?;
    eprintln!(
        "seed: {} documents in {:.1}s",
        bulk.len(),
        started.elapsed().as_secs_f64()
    );

    // The commit profile's targets: one document per worker, fresh, so their
    // revisions start at a number this file can state.
    let mut targets = Vec::new();
    for worker in 0..plan.workers {
        let created = command(
            &mut conn,
            &owner.token,
            "create-document",
            json!({ "title": format!("commit target {worker}"), "body": "seed" }),
        )
        .await?;
        targets.push(json!({
            "document": created["document"].as_str().unwrap_or_default(),
            "rev": 1,
        }));
    }

    // The fan-out document, and the subscribers who can see it. Each is its
    // own identity: the socket cap is per identity (16), so one identity
    // cannot hold a hundred sockets and pretending otherwise would measure
    // the refusal.
    let fanout = command(
        &mut conn,
        &owner.token,
        "create-document",
        json!({ "title": "fan-out probe", "body": "seed" }),
    )
    .await?;
    let fanout_id = fanout["document"]
        .as_str()
        .ok_or_else(|| format!("create-document named no document: {fanout}"))?
        .to_owned();

    let mut subscribers = Vec::new();
    for index in 0..plan.subscribers {
        let caller = register(
            &mut conn,
            &format!("Perf Subscriber {index}"),
            &format!("perf-sub-{stamp}-{index}@perf.invalid"),
        )
        .await?;
        command(
            &mut conn,
            &owner.token,
            "share",
            json!({ "resource": fanout_id, "subject": caller.person, "relation": "viewer" }),
        )
        .await?;
        subscribers.push(json!({ "token": caller.token, "person": caller.person }));
    }
    eprintln!("seed: {} subscribers hold the fan-out document", plan.subscribers);

    let seed = json!({
        "host": plan.host,
        "stamp": stamp,
        "owner": { "token": owner.token, "person": owner.person },
        "documents": plan.documents,
        "fanout": { "document": fanout_id, "rev": 1 },
        "targets": targets,
        "subscribers": subscribers,
        "seconds": started.elapsed().as_secs_f64(),
    });
    std::fs::write(&plan.out, serde_json::to_string_pretty(&seed).unwrap_or_default())
        .map_err(|error| format!("write {}: {error}", plan.out))?;
    // oha takes its cookie on a command line, and a shell that had to dig the
    // token out of the JSON would end up echoing it. One file, one line, no
    // parsing — and it goes away with the rest of the run's state.
    let beside = std::path::Path::new(&plan.out).with_file_name("owner.cookie");
    std::fs::write(&beside, &owner.token)
        .map_err(|error| format!("write {}: {error}", beside.display()))?;
    eprintln!(
        "seed: wrote {} in {:.1}s",
        plan.out,
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

/// Create `count` documents across eight connections.
async fn fanned(
    plan: &Plan,
    token: &str,
    stamp: &str,
    count: usize,
) -> Result<Vec<String>, String> {
    const LANES: usize = 8;
    let mut tasks = Vec::new();
    for lane in 0..LANES {
        let host = plan.host.clone();
        let token = token.to_owned();
        let stamp = stamp.to_owned();
        let mine = (count / LANES) + usize::from(lane < count % LANES);
        tasks.push(tokio::spawn(async move {
            let mut conn = Conn::connect(&host)
                .await
                .map_err(|error| format!("connect {host}: {error}"))?;
            let mut ids = Vec::with_capacity(mine);
            for index in 0..mine {
                let created = command(
                    &mut conn,
                    &token,
                    "create-document",
                    json!({
                        "title": format!("perf {stamp} {lane}-{index}"),
                        "body": "One of ten thousand, so the list has something to be long about.",
                    }),
                )
                .await?;
                if let Some(id) = created["document"].as_str() {
                    ids.push(id.to_owned());
                }
            }
            Ok::<Vec<String>, String>(ids)
        }));
    }
    let mut all = Vec::with_capacity(count);
    for task in tasks {
        all.extend(task.await.map_err(|error| error.to_string())??);
    }
    Ok(all)
}

/// Percent-encode a form field. The alphabet is small and known: the harness
/// only ever sends names, addresses and one password.
fn urlencode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

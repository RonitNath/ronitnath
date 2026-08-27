//! The commit profile: `POST /api/cmd/edit-document` mixed with reads.
//!
//! oha cannot drive this one. Two of the command contract's rules make a fixed
//! request body meaningless — a repeated idempotency key replays instead of
//! committing, and `expected_rev` is a lost-update guard, so the second
//! identical edit is a 403 rather than a write. So each worker holds its own
//! document, mints its own key, and carries its own revision forward, and the
//! read it interleaves is the same `/api/q/documents` the list profile
//! measures — because a commit path measured with the reads switched off is
//! not the path the product runs.
//!
//! The same profile is the drill's load. `--timeline` turns on a per-second
//! record of what completed and what refused, which is how a stall gets a
//! duration instead of an adjective: the seconds a killed voter costs are the
//! seconds with no completed write in them.

use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::http::Conn;
use crate::key;
use crate::stats::Samples;

pub struct Plan {
    pub host: String,
    pub seed: Value,
    pub seconds: u64,
    /// Reads per write. Four is the shape a document tier actually has.
    pub reads: u32,
    /// Record a per-second timeline of writes — the drill's evidence.
    pub timeline: bool,
    /// Keep going after a refusal instead of stopping. The drill needs this:
    /// two voters down means every write refuses, and the profile has to
    /// still be running when they come back.
    pub persist: bool,
}

/// How many refusals in a row mean something other than a stale revision.
const GIVE_UP: u32 = 200;

/// One completed write, as the timeline remembers it.
struct Mark {
    at: f64,
    took: Duration,
    status: u16,
}

pub async fn run(plan: Plan) -> Result<Value, String> {
    let token = plan.seed["owner"]["token"]
        .as_str()
        .ok_or_else(|| "the seed names no owner token".to_owned())?
        .to_owned();
    let targets: Vec<(String, u64)> = plan.seed["targets"]
        .as_array()
        .ok_or_else(|| "the seed names no commit targets".to_owned())?
        .iter()
        .map(|target| {
            (
                target["document"].as_str().unwrap_or_default().to_owned(),
                target["rev"].as_u64().unwrap_or(1),
            )
        })
        .collect();
    if targets.is_empty() {
        return Err("the seed names no commit targets".to_owned());
    }

    let started = Instant::now();
    let deadline = started + Duration::from_secs(plan.seconds);
    let mut tasks = Vec::new();
    for (document, rev) in targets {
        let worker = Worker {
            host: plan.host.clone(),
            token: token.clone(),
            document,
            rev,
            started,
            deadline,
            reads: plan.reads,
            persist: plan.persist,
        };
        tasks.push(tokio::spawn(worker.run()));
    }

    let (mut writes, mut lists, mut marks) = (Samples::default(), Samples::default(), Vec::new());
    for task in tasks {
        let (w, l, m) = task.await.map_err(|error| error.to_string())?;
        writes.absorb(w);
        lists.absorb(l);
        marks.extend(m);
    }
    let wall = started.elapsed();

    let write = writes.summary(wall);
    let list = lists.summary(wall);
    println!("{}", write.line("POST /api/cmd/edit-document"));
    println!("{}", list.line("GET /api/q/documents (mixed)"));
    let mut report = json!({
        "wall_seconds": wall.as_secs_f64(),
        "profiles": [
            write.json("POST /api/cmd/edit-document"),
            list.json("GET /api/q/documents (mixed)"),
        ],
    });
    if plan.timeline {
        let timeline = bucket(&mut marks);
        for line in timeline.1 {
            println!("{line}");
        }
        report["timeline"] = timeline.0;
    }
    Ok(report)
}

/// One second per row: how many writes committed, how many refused, and the
/// slowest one in it.
fn bucket(marks: &mut Vec<Mark>) -> (Value, Vec<String>) {
    marks.sort_by(|a, b| a.at.total_cmp(&b.at));
    let mut rows: Vec<Value> = Vec::new();
    let mut lines = Vec::new();
    let last = marks.last().map_or(0, |mark| mark.at as u64);
    for second in 0..=last {
        let (mut ok, mut refused, mut worst) = (0u64, 0u64, 0.0f64);
        for mark in marks.iter().filter(|mark| mark.at as u64 == second) {
            if mark.status == 200 {
                ok += 1;
            } else {
                refused += 1;
            }
            worst = worst.max(mark.took.as_secs_f64() * 1000.0);
        }
        rows.push(json!({ "second": second, "committed": ok, "refused": refused, "slowest_ms": worst }));
        lines.push(format!(
            "  t+{second:<4} committed {ok:<6} refused {refused:<6} slowest {worst:>9.1} ms"
        ));
    }
    (Value::Array(rows), lines)
}

struct Worker {
    host: String,
    token: String,
    document: String,
    rev: u64,
    started: Instant,
    deadline: Instant,
    reads: u32,
    persist: bool,
}

impl Worker {
    async fn run(mut self) -> (Samples, Samples, Vec<Mark>) {
        let (mut writes, mut lists, mut marks) =
            (Samples::default(), Samples::default(), Vec::new());
        let mut conn = None;
        let mut refusals = 0u32;

        while Instant::now() < self.deadline {
            if conn.is_none() {
                match Conn::connect(&self.host).await {
                    Ok(fresh) => conn = Some(fresh),
                    Err(_) => {
                        writes.fail();
                        if !self.persist {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        continue;
                    }
                }
            }
            let socket = conn.as_mut().expect("a connection");

            let body = json!({
                "key": key::uuid(),
                "document": self.document,
                "expected_rev": self.rev,
                "body": format!("revision {}, written by the commit profile", self.rev),
            })
            .to_string();
            match socket
                .post(
                    "/api/cmd/edit-document",
                    Some(&self.token),
                    ("application/json", &body),
                )
                .await
            {
                Ok(reply) => {
                    writes.push(reply.took, reply.status);
                    marks.push(Mark {
                        at: self.started.elapsed().as_secs_f64(),
                        took: reply.took,
                        status: reply.status,
                    });
                    if reply.status == 200 {
                        // The reply says which revision it produced; trusting
                        // it rather than incrementing keeps the worker correct
                        // when a request committed and its answer was lost.
                        self.rev = reply
                            .json()
                            .ok()
                            .and_then(|value| value["result"]["rev"].as_u64())
                            .unwrap_or(self.rev + 1);
                        refusals = 0;
                    } else {
                        // A decline says the revision this worker believes in
                        // is not the one the node it asked can see — which
                        // happens without any node being down, because the
                        // command's precondition is a *local* read and a
                        // follower may not have applied the last entry yet.
                        // So the worker asks what the revision is now rather
                        // than guessing, and keeps going: the profile is
                        // about throughput and latency, and the declines are
                        // counted where the table can show them.
                        refusals += 1;
                        if refusals > GIVE_UP {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(if self.persist {
                            100
                        } else {
                            5
                        }))
                        .await;
                        if let Some(seen) = self.reread(&self.token).await {
                            self.rev = seen;
                        }
                    }
                }
                Err(_) => {
                    writes.fail();
                    marks.push(Mark {
                        at: self.started.elapsed().as_secs_f64(),
                        took: Duration::ZERO,
                        status: 0,
                    });
                    conn = None;
                    if !self.persist {
                        break;
                    }
                    continue;
                }
            }

            for _ in 0..self.reads {
                if Instant::now() >= self.deadline {
                    break;
                }
                let socket = conn.as_mut().expect("a connection");
                match socket.get("/api/q/documents", Some(&self.token)).await {
                    Ok(reply) => lists.push(reply.took, reply.status),
                    Err(_) => {
                        lists.fail();
                        conn = None;
                        break;
                    }
                }
            }
        }
        (writes, lists, marks)
    }

    /// Ask the survivor what revision the document is at now.
    async fn reread(&self, token: &str) -> Option<u64> {
        let mut conn = Conn::connect(&self.host).await.ok()?;
        let reply = conn
            .get(
                &format!("/api/q/document?id={}", self.document),
                Some(token),
            )
            .await
            .ok()?;
        let value = reply.json().ok()?;
        // `/api/q/document` answers with the query's rows — an array of one.
        value
            .as_array()
            .and_then(|rows| rows.first())
            .and_then(|row| row["draft_rev"].as_u64())
    }
}

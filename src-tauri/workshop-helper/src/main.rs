// SPDX-License-Identifier: GPL-3.0-or-later
//! Subscribes and unsubscribes Steam Workshop items on behalf of Balota.
//!
//! This lives in its own short-lived process on purpose. Initialising the
//! Steamworks SDK means identifying as AppID 221100, which makes Steam report
//! the user as *playing DayZ* for as long as the process is alive — so it runs
//! for a few seconds and exits. It also keeps the SDK, and any crash inside it,
//! out of the launcher itself.
//!
//! Usage:  balota-workshop <subscribe|unsubscribe|state|info> <id> [id …]
//! Output: one JSON object on stdout.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use steamworks::{Client, ItemState, PublishedFileId};

const DAYZ_APP_ID: u32 = 221100;
/// Generous enough for a 50-mod server, short enough not to hang the UI.
const TIMEOUT: Duration = Duration::from_secs(90);
/// A details query is one round trip; it either answers quickly or not at all.
const INFO_TIMEOUT: Duration = Duration::from_secs(20);
/// Steam's own cap on CreateQueryUGCDetailsRequest.
const MAX_QUERY_BATCH: usize = 100;

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct Outcome {
    ok: Vec<u64>,
    failed: Vec<Failure>,
    timed_out: Vec<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct Failure {
    id: u64,
    error: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StateItem {
    id: u64,
    subscribed: bool,
    installed: bool,
}

#[derive(Serialize)]
struct StateReply {
    items: Vec<StateItem>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModInfo {
    id: u64,
    title: String,
    /// Download size in bytes, 0 when Steam does not report one.
    size_bytes: u64,
    /// True when the size came from the copy already on disk.
    installed: bool,
}

#[derive(Serialize)]
struct InfoReply {
    items: Vec<ModInfo>,
    /// IDs Steam had nothing to say about — removed from the Workshop, most
    /// likely. Worth surfacing: those are the ones that never download.
    unknown: Vec<u64>,
}

fn emit<T: Serialize>(value: &T) {
    // Hand-rolled JSON only escaped `\` and `"`; a SteamError carrying a
    // newline produced something the launcher could not parse.
    match serde_json::to_string(value) {
        Ok(json) => println!("{json}"),
        Err(e) => println!(r#"{{"ok":[],"failed":[],"timedOut":[],"error":"{e}"}}"#),
    }
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    emit(&Outcome {
        error: Some(message.to_string()),
        ..Default::default()
    });
    std::process::exit(1);
}

fn main() {
    let mut args = std::env::args().skip(1);

    let action = args.next().unwrap_or_default();
    if !matches!(
        action.as_str(),
        "subscribe" | "unsubscribe" | "state" | "info"
    ) {
        fail("usage: balota-workshop <subscribe|unsubscribe|state|info> <id> [id …]");
    }

    let ids: Vec<u64> = args.filter_map(|a| a.trim().parse().ok()).collect();
    if ids.is_empty() {
        fail("no valid Workshop IDs given");
    }

    // Steam must be running; without it there is nothing to talk to.
    let client = match Client::init_app(DAYZ_APP_ID) {
        Ok(client) => client,
        Err(e) => fail(&format!(
            "Steam is not running or the SDK failed to start: {e}"
        )),
    };

    let ugc = client.ugc();

    // `state` is a plain query: no callbacks, no waiting. It is what tells an
    // orphaned download (on disk, unknown to Steam) apart from a real
    // subscription.
    if action == "state" {
        let items = ids
            .iter()
            .map(|&id| {
                let state = ugc.item_state(PublishedFileId(id));
                StateItem {
                    id,
                    subscribed: state.contains(ItemState::SUBSCRIBED),
                    installed: state.contains(ItemState::INSTALLED),
                }
            })
            .collect();

        emit(&StateReply { items });
        return;
    }

    // `info` answers "how much am I about to download?" before the user
    // commits to it. A 40-mod server can be several gigabytes, and until now
    // the only way to find out was to watch it happen.
    if action == "info" {
        let mut items: Vec<ModInfo> = Vec::new();
        let mut seen: BTreeSet<u64> = BTreeSet::new();

        for chunk in ids.chunks(MAX_QUERY_BATCH) {
            // Anything already on disk needs no round trip, and its real size
            // beats the Workshop's compressed figure.
            let mut to_query: Vec<PublishedFileId> = Vec::new();
            for &id in chunk {
                match ugc.item_install_info(PublishedFileId(id)) {
                    Some(info) => {
                        seen.insert(id);
                        items.push(ModInfo {
                            id,
                            title: String::new(),
                            size_bytes: info.size_on_disk,
                            installed: true,
                        });
                    }
                    None => to_query.push(PublishedFileId(id)),
                }
            }
            if to_query.is_empty() {
                continue;
            }

            let Ok(query) = ugc.query_items(to_query) else {
                continue;
            };

            let found: Arc<Mutex<Vec<ModInfo>>> = Arc::new(Mutex::new(Vec::new()));
            let settled = Arc::new(AtomicBool::new(false));

            query.fetch({
                let found = found.clone();
                let settled = settled.clone();
                move |result| {
                    if let Ok(list) = result {
                        for index in 0..list.returned_results() {
                            if let Some(entry) = list.get(index) {
                                found.lock().unwrap().push(ModInfo {
                                    id: entry.published_file_id.0,
                                    title: entry.title,
                                    // file_size is a u32, so anything past 4 GiB
                                    // comes back truncated. Nothing to do about
                                    // that here beyond not pretending otherwise.
                                    size_bytes: u64::from(entry.file_size),
                                    installed: false,
                                });
                            }
                        }
                    }
                    settled.store(true, Ordering::SeqCst);
                }
            });

            let deadline = Instant::now() + INFO_TIMEOUT;
            while !settled.load(Ordering::SeqCst) && Instant::now() < deadline {
                client.run_callbacks();
                std::thread::sleep(Duration::from_millis(50));
            }

            for info in found.lock().unwrap().drain(..) {
                seen.insert(info.id);
                items.push(info);
            }
        }

        let unknown: Vec<u64> = ids
            .iter()
            .copied()
            .filter(|id| !seen.contains(id))
            .collect();
        emit(&InfoReply { items, unknown });
        return;
    }

    let subscribing = action == "subscribe";
    let done: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
    let failed: Arc<Mutex<Vec<(u64, String)>>> = Arc::new(Mutex::new(Vec::new()));

    for &id in &ids {
        let done = done.clone();
        let failed = failed.clone();
        let file = PublishedFileId(id);

        let callback = move |result: Result<(), steamworks::SteamError>| match result {
            Ok(()) => done.lock().unwrap().push(id),
            Err(e) => failed.lock().unwrap().push((id, e.to_string())),
        };

        if subscribing {
            ugc.subscribe_item(file, callback);
        } else {
            ugc.unsubscribe_item(file, callback);
        }
    }

    let deadline = Instant::now() + TIMEOUT;
    loop {
        client.run_callbacks();

        let settled = done.lock().unwrap().len() + failed.lock().unwrap().len();
        if settled >= ids.len() || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let accepted = done.lock().unwrap().clone();
    let mut failed = failed.lock().unwrap().clone();

    // Steam accepting the call is not the same as the item existing. An item
    // that has been removed from the Workshop reports success and then never
    // arrives, which left the launcher waiting on a download forever.
    let mut ok = Vec::new();
    if subscribing {
        for id in accepted {
            let state = ugc.item_state(PublishedFileId(id));
            if state.contains(ItemState::SUBSCRIBED) {
                ok.push(id);
            } else {
                failed.push((
                    id,
                    "Steam accepted the subscription but is not tracking the item; it has \
                     probably been removed from the Workshop."
                        .to_string(),
                ));
            }
        }
    } else {
        ok = accepted;
    }

    // Anything neither confirmed nor rejected ran out of time.
    let settled: BTreeSet<u64> = ok
        .iter()
        .chain(failed.iter().map(|(id, _)| id))
        .copied()
        .collect();
    let timed_out: Vec<u64> = ids
        .iter()
        .copied()
        .filter(|id| !settled.contains(id))
        .collect();

    emit(&Outcome {
        ok,
        failed: failed
            .into_iter()
            .map(|(id, error)| Failure { id, error })
            .collect(),
        timed_out,
        error: None,
    });
}

//! Subscribes and unsubscribes Steam Workshop items on behalf of Balota.
//!
//! This lives in its own short-lived process on purpose. Initialising the
//! Steamworks SDK means identifying as AppID 221100, which makes Steam report
//! the user as *playing DayZ* for as long as the process is alive — so it runs
//! for a few seconds and exits. It also keeps the SDK, and any crash inside it,
//! out of the launcher itself.
//!
//! Usage:  balota-workshop <subscribe|unsubscribe> <id> [id …]
//! Output: one JSON object on stdout.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use steamworks::{Client, PublishedFileId};

const DAYZ_APP_ID: u32 = 221100;
/// Generous enough for a 50-mod server, short enough not to hang the UI.
const TIMEOUT: Duration = Duration::from_secs(90);

fn main() {
    let mut args = std::env::args().skip(1);

    let action = args.next().unwrap_or_default();
    if !matches!(action.as_str(), "subscribe" | "unsubscribe" | "state") {
        fail("usage: balota-workshop <subscribe|unsubscribe|state> <id> [id …]");
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
        let items: Vec<String> = ids
            .iter()
            .map(|&id| {
                let state = ugc.item_state(PublishedFileId(id));
                format!(
                    "{{\"id\":{id},\"subscribed\":{},\"installed\":{}}}",
                    state.contains(steamworks::ItemState::SUBSCRIBED),
                    state.contains(steamworks::ItemState::INSTALLED),
                )
            })
            .collect();

        println!("{{\"items\":[{}]}}", items.join(","));
        return;
    }
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

        if action == "subscribe" {
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

    let done = done.lock().unwrap().clone();
    let failed = failed.lock().unwrap().clone();

    // Anything neither confirmed nor rejected ran out of time.
    let settled: BTreeSet<u64> = done
        .iter()
        .chain(failed.iter().map(|(id, _)| id))
        .copied()
        .collect();
    let timed_out: Vec<u64> = ids
        .iter()
        .copied()
        .filter(|id| !settled.contains(id))
        .collect();

    println!(
        "{{\"ok\":[{}],\"failed\":[{}],\"timedOut\":[{}]}}",
        join(&done),
        failed
            .iter()
            .map(|(id, e)| format!("{{\"id\":{id},\"error\":\"{}\"}}", escape(e)))
            .collect::<Vec<_>>()
            .join(","),
        join(&timed_out),
    );
}

fn join(ids: &[u64]) -> String {
    ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",")
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    println!(
        "{{\"ok\":[],\"failed\":[],\"timedOut\":[],\"error\":\"{}\"}}",
        escape(message)
    );
    std::process::exit(1);
}

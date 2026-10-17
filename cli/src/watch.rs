//! `watch`: packs and checks a Cartridge directory on every save, printing each result. Nothing is uploaded. Upload
//! stays manual, through the Chittr composer.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use notify::event::EventKind;
use notify::{Event, RecursiveMode, Watcher};

use crate::{pack, validate};

/// How long the directory must stay quiet before it is packed, so one save that writes several files packs once.
const DEBOUNCE: Duration = Duration::from_millis(250);

/// Packs and checks `dir` now, then again after each change. Runs until interrupted.
pub fn run(dir: &Path) -> anyhow::Result<()> {
    let dir = dir.canonicalize()?;
    let out = pack::default_out(&dir);
    let (sender, receiver) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        let _ = sender.send(event);
    })?;
    watcher.watch(&dir, RecursiveMode::Recursive)?;
    println!("watching {} (Ctrl-C to stop)", dir.display());
    cycle(&dir, &out);
    loop {
        let first = receiver
            .recv()
            .map_err(|_| anyhow::anyhow!("the watcher stopped"))?;
        if !is_relevant(&first, &dir, &out) {
            continue;
        }
        let mut deadline = Instant::now() + DEBOUNCE;
        loop {
            match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(_) => deadline = Instant::now() + DEBOUNCE,
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
        }
        cycle(&dir, &out);
    }
}

/// A save is relevant unless it is only a read, the output archive, or a hidden file such as an editor's swap file.
fn is_relevant(event: &notify::Result<Event>, dir: &Path, out: &Path) -> bool {
    let Ok(event) = event else {
        return true;
    };
    if matches!(event.kind, EventKind::Access(_)) {
        return false;
    }
    event.paths.iter().any(|path| counts(path, dir, out))
}

fn counts(path: &PathBuf, dir: &Path, out: &Path) -> bool {
    if path == out {
        return false;
    }
    let Ok(relative) = path.strip_prefix(dir) else {
        return false;
    };
    let hidden = relative
        .components()
        .any(|part| part.as_os_str().to_string_lossy().starts_with('.'));
    !hidden
        && path
            .extension()
            .is_none_or(|extension| extension != "cartridge")
}

/// One pack and check. A valid archive is written to `out`; an invalid one is reported and not written.
fn cycle(dir: &Path, out: &Path) {
    let stamp = clock();
    let outcome = pack::collect(dir)
        .and_then(|files| pack::zip(&files))
        .map_err(|error| error.to_string());
    match outcome {
        Err(message) => println!("[{stamp}] {message}"),
        Ok(bytes) => match validate::validate(&bytes) {
            Ok(cartridge) => match std::fs::write(out, &bytes) {
                Ok(()) => {
                    println!(
                        "[{stamp}] ok: {} {} -> {}",
                        cartridge.manifest.name,
                        cartridge.manifest.version,
                        out.display()
                    );
                    for warning in &cartridge.warnings {
                        println!("[{stamp}] warning: {warning}");
                    }
                }
                Err(error) => println!("[{stamp}] could not write {}: {error}", out.display()),
            },
            Err(error) => println!("[{stamp}] {error}"),
        },
    }
}

/// The current UTC time of day, as HH:MM:SS.
fn clock() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        % 86_400;
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

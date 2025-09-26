use std::path::Path;

use inotify::{EventMask, Inotify, WatchMask};

pub async fn monitor_dirs<P: AsRef<Path> + std::fmt::Debug>(dirs: Vec<P>) {
    let mut inotify = Inotify::init().expect("Failed to initialize inotify");

    for dir in &dirs {
        inotify
            .watches()
            .add(dir.as_ref(), WatchMask::CREATE)
            .expect("Failed to add inotify watch");
    }

    tracing::info!(dirs=?dirs, "Watching directories for activity");

    let mut buffer = [0u8; 4096];
    loop {
        tokio::task::yield_now().await;

        let Ok(events) = inotify.read_events(&mut buffer) else {
            continue;
        };

        for event in events {
            if event.mask.contains(EventMask::CREATE) {
                if event.mask.contains(EventMask::ISDIR) {
                    tracing::info!(event=?event.name,"Directory created");
                } else {
                    tracing::info!(event=?event.name,"File created");
                }
            }
        }

    }
}

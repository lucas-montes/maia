use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use inotify::{EventMask, Inotify, WatchMask};

use crate::{receipts::handle_receipt_file, state::State};

#[derive(Debug)]
pub enum WatchedDirKind {
    Receipt,
    Bank,
    Investment,
}

#[derive(Debug)]
pub struct WatchedDir {
    path: PathBuf,
    kind: WatchedDirKind,
}

impl WatchedDir {
    pub fn new(path: PathBuf, kind: WatchedDirKind) -> Self {
        Self { path, kind }
    }

    pub fn kind(&self) -> &WatchedDirKind {
        &self.kind
    }
}

impl AsRef<Path> for WatchedDir {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

pub async fn monitor_dirs(state: State) {
    let mut inotify = Inotify::init().expect("Failed to initialize inotify");

    let wd_to_dir: HashMap<i32, WatchedDir> = state
        .config()
        .watched_dirs()
        .into_iter()
        .map(|dir| {
            let fd = inotify
                .watches()
                .add(dir.as_ref(), WatchMask::CREATE)
                .expect("Failed to add inotify watch");
            //TODO: probably handle the error

            (fd.get_watch_descriptor_id(), dir)
        })
        .collect();

    let mut buffer = [0u8; 4096];
    loop {
        tokio::task::yield_now().await;

        let Ok(events) = inotify.read_events(&mut buffer) else {
            continue;
        };

        for event in events {
            if event.mask.contains(EventMask::CREATE) {
                if event.mask.contains(EventMask::ISDIR) {
                    //NOTE: it doesnt listen for files inside the dir
                    tracing::debug!(event=?event.name,"Directory created");
                } else {
                    if let Some(dir) = wd_to_dir.get(&event.wd.get_watch_descriptor_id()) {
                        if let Some(name) = event.name {
                            let file_path = dir.path.join(name);
                            tracing::info!(file=?file_path, "File created in");
                            match dir.kind() {
                                WatchedDirKind::Receipt => {
                                    handle_receipt_file(&state, file_path).await
                                }
                                WatchedDirKind::Bank => handle_bank_file(&state, file_path).await,
                                WatchedDirKind::Investment => {
                                    handle_investment_file(&state, file_path).await
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn handle_bank_file(state: &State, file: PathBuf) {
    tracing::info!(file=?file, "Handling bank statement file");
    // TODO: implement bank statement parsing logic
}

async fn handle_investment_file(state: &State, file: PathBuf) {
    tracing::info!(file=?file, "Handling investment statement file");
    // TODO: implement investment statement parsing logic
}

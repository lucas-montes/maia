use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, OnceLock},
};

pub type LogBuffer = Arc<Mutex<VecDeque<String>>>;

static GLOBAL: OnceLock<LogBuffer> = OnceLock::new();

pub fn global_buffer() -> LogBuffer {
    GLOBAL.get_or_init(|| new_buffer(200)).clone()
}

pub fn new_buffer(cap: usize) -> LogBuffer {
    Arc::new(Mutex::new(VecDeque::with_capacity(cap)))
}

pub fn push_log(buf: &LogBuffer, line: String) {
    let mut q = buf.lock().unwrap();
    if q.len() >= 200 {
        q.pop_front();
    }
    q.push_back(line.clone());
    if let Some(g) = GLOBAL.get() {
        if !Arc::ptr_eq(g, buf) {
            let mut gg = g.lock().unwrap();
            if gg.len() >= 200 {
                gg.pop_front();
            }
            gg.push_back(line);
        }
    } else {
        let _ = GLOBAL.set(buf.clone());
    }
}

pub fn tail_logs(buf: &LogBuffer, limit: usize) -> Vec<String> {
    let q = buf.lock().unwrap();
    let len = q.len();
    let start = len.saturating_sub(limit);
    q.iter().skip(start).cloned().collect()
}

pub fn tail_global(limit: usize) -> Vec<String> {
    if let Some(g) = GLOBAL.get() {
        tail_logs(g, limit)
    } else {
        Vec::new()
    }
}

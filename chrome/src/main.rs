use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, Read, Write};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
struct UrlEntry {
    url: String,
    content_path: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Db {
    entries: Vec<UrlEntry>,
    db_path: PathBuf,
}

impl Db {
    fn load(db_path: PathBuf) -> io::Result<Self> {
        if db_path.exists() {
            let data = fs::read_to_string(&db_path)?;
            let mut db: Db = serde_json::from_str(&data)?;
            db.db_path = db_path;
            Ok(db)
        } else {
            Ok(Db {
                entries: vec![],
                db_path,
            })
        }
    }

    fn save(&self) -> io::Result<()> {
        let data = serde_json::to_string_pretty(&self)?;
        fs::write(&self.db_path, data)
    }

    fn add_entry(&mut self, url: String, content_path: Option<PathBuf>) -> io::Result<()> {
        self.entries.push(UrlEntry { url, content_path });
        self.save()
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", content = "data")]
enum BrowserAction {
    ProcessUrl { url: String },
    ProcessUrlAndContent { url: String, content: String },
}

impl BrowserAction {
    fn run(self, db: &mut Db) -> BrowserResponse {
        let result = match self {
            BrowserAction::ProcessUrl { url } => db.add_entry(url, None),
            BrowserAction::ProcessUrlAndContent { url, content } => {
                match save_url_content(&url, &content) {
                    Ok(path) => db.add_entry(url, Some(PathBuf::from(path))),
                    Err(e) => Err(e),
                }
            }
        };

        match result {
            Ok(_) => BrowserResponse::success("Action completed successfully".to_string()),
            Err(e) => BrowserResponse::error(format!("Action failed: {}", e)),
        }
    }
}

#[derive(Debug, Serialize)]
enum BrowserResponseStatus {
    Success,
    Error,
}

#[derive(Debug, Serialize)]
struct BrowserResponse {
    status: BrowserResponseStatus,
    message: String,
}

impl BrowserResponse {
    fn success(message: String) -> Self {
        BrowserResponse {
            status: BrowserResponseStatus::Success,
            message,
        }
    }

    fn error(message: String) -> Self {
        BrowserResponse {
            status: BrowserResponseStatus::Error,
            message,
        }
    }

    fn send(&self, stdout: &mut io::Stdout) -> io::Result<()> {
        //TODO: convert the struct into bytes wihtout the intermediary string
        let response = serde_json::to_string(self)?;
        let length = response.len() as u32;
        stdout.write_all(&length.to_le_bytes())?;
        stdout.write_all(response.as_bytes())?;
        stdout.flush()
    }
}

fn save_url_content(url: &str, content: &str) -> io::Result<String> {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let url_hash = hasher.finish();
    let file_path = format!("data/output_{:x}.html", url_hash);
    let mut file = File::create(&file_path)?;
    file.write_all(content.as_bytes())?;
    Ok(file_path)
}

fn main() -> io::Result<()> {
    //TODO: it should write to a file
    tracing_subscriber::fmt::init();

    let mut stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut db = Db::load(PathBuf::from("db.json"))?;

    loop {
        // Read message length (4 bytes, u32 little-endian)
        // TODO: maybe this is not the best approach, check if we need to keep looping or how to handle better the connection
        let mut length_bytes = [0u8; 4];
        if stdin.read_exact(&mut length_bytes).is_err() {
            continue;
        }
        let length = u32::from_le_bytes(length_bytes) as usize;

        // Read the message
        let mut message_bytes = vec![0u8; length];
        if stdin.read_exact(&mut message_bytes).is_err() {
            tracing::error!("Failed to read message");
            break;
        }

        // Parse JSON
        let message: BrowserAction = match serde_json::from_slice(&message_bytes) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(err=%e, "JSON parse error");
                BrowserResponse::error(e.to_string()).send(&mut stdout)?;
                continue;
            }
        };

        message.run(&mut db).send(&mut stdout)?;
    }

    Ok(())
}

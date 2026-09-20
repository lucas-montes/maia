use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

pub fn seed_exercises(conn: &Connection, data_dir: &Path) -> anyhow::Result<usize> {
    let parsed = data_dir.join("parsed");
    let images = data_dir.join("images");
    let videos = data_dir.join("videos");
    let media_dir = conn
        .query_row("PRAGMA database_list", [], |_| Ok(()))
        .ok();
    let db_path = data_dir.join("../maia.db");
    let media_base = db_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("exercise_media");
    let media_base = if data_dir.join("parsed").exists() {
        data_dir
            .parent()
            .unwrap_or(Path::new("."))
            .join("exercise_media")
    } else {
        media_base
    };
    let _ = std::fs::create_dir_all(&media_base);
    seed_exercises_to(conn, &parsed, &images, &videos, &media_base)
}

pub fn seed_exercises_to(
    conn: &Connection,
    parsed_dir: &Path,
    images_dir: &Path,
    videos_dir: &Path,
    media_dir: &Path,
) -> anyhow::Result<usize> {
    std::fs::create_dir_all(media_dir).ok();
    let mut count = 0;
    let entries = std::fs::read_dir(parsed_dir)?;
    let now = crate::db::server_time_ms();
    let tx = conn.unchecked_transaction()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let data = match std::fs::read_to_string(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let v: serde_json::Value = match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let name = v.get("name").and_then(|x| x.as_str()).unwrap_or(&stem).to_string();
        let body_part = v.get("body_part").and_then(|x| x.as_str()).map(|s| s.to_string());
        let equipment = v.get("equipment").and_then(|x| x.as_str()).map(|s| s.to_string());
        let primary = v.get("primary").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let secondary = v.get("secondary").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let instructions = v.get("instructions").map(|x| x.to_string()).unwrap_or("[]".into());
        let tips = v.get("tips").map(|x| x.to_string()).unwrap_or("[]".into());
        let faqs = v.get("faqs").and_then(|x| x.as_str()).map(|s| s.to_string());
        let keywords = v.get("keywords").map(|x| x.to_string());
        let exercise_type = body_part.clone().unwrap_or_else(|| "strength".to_string());
        let has_image = images_dir.join(format!("{stem}.png")).exists()
            || images_dir.join(format!("{stem}.jpg")).exists();
        let has_video = videos_dir.join(format!("{stem}.mp4")).exists();
        let image_path = if has_image {
            Some(format!("exercise_media/{stem}.jpg"))
        } else {
            None
        };
        let video_path = if has_video {
            Some(format!("exercise_media/{stem}.mp4"))
        } else {
            None
        };
        if has_image {
            let src = if images_dir.join(format!("{stem}.png")).exists() {
                images_dir.join(format!("{stem}.png"))
            } else {
                images_dir.join(format!("{stem}.jpg"))
            };
            let dst = media_dir.join(format!("{stem}.jpg"));
            if !dst.exists() {
                let _ = std::fs::copy(&src, &dst);
            }
        }
        if has_video {
            let src = videos_dir.join(format!("{stem}.mp4"));
            let dst = media_dir.join(format!("{stem}.mp4"));
            if !dst.exists() {
                let _ = std::fs::copy(&src, &dst);
            }
        }
        let _ = conn.execute(
            "INSERT INTO exercises (id, name, exercise_type, body_part, equipment, primary_muscle, secondary_muscle, instructions, tips, faqs, keywords, image_path, video_path, is_canonical, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,1,?14,?15) ON CONFLICT(id) DO UPDATE SET name=excluded.name, exercise_type=excluded.exercise_type, body_part=excluded.body_part, equipment=excluded.equipment, primary_muscle=excluded.primary_muscle, secondary_muscle=excluded.secondary_muscle, instructions=excluded.instructions, tips=excluded.tips, faqs=excluded.faqs, keywords=excluded.keywords, image_path=excluded.image_path, video_path=excluded.video_path, updated_at=excluded.updated_at",
            params![stem, name, exercise_type, body_part, equipment, primary, secondary, instructions, tips, faqs, keywords, image_path, video_path, now, now],
        );
        count += 1;
    }
    tx.commit()?;
    Ok(count)
}

pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod handlers;
pub mod server;

pub use config::Config;
pub use server::{create_router, start_server, AppState};

#[cfg(test)]
mod tests {
    use super::server::start_test_server;

    #[tokio::test]
    async fn health_returns_ok() {
        let (port, handle) = start_test_server().await;
        let url = format!("http://127.0.0.1:{port}/health");
        let resp = reqwest::get(&url).await.expect("health request");
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["status"], "ok");
        handle.abort();
    }

    #[tokio::test]
    async fn auth_bearer_rejected_and_accepted() {
        let (port, handle) = start_test_server().await;
        let base = format!("http://127.0.0.1:{port}");
        let client = reqwest::Client::new();

        let resp = client.get(format!("{base}/protected")).send().await.unwrap();
        assert_eq!(resp.status(), 401);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert!(body["message"].as_str().unwrap().contains("Missing"));

        let resp = client
            .get(format!("{base}/protected"))
            .header("Authorization", "Bearer wrong-key")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 401);

        let resp = client
            .get(format!("{base}/protected"))
            .header("Authorization", "Bearer test-key")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        let resp = client
            .get(format!("{base}/health"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        handle.abort();
    }

    #[tokio::test]
    async fn cors_headers_present() {
        let (port, handle) = start_test_server().await;
        let url = format!("http://127.0.0.1:{port}/health");
        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .header("Origin", "http://tauri.localhost")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        handle.abort();
    }

    #[tokio::test]
    async fn pull_exercises_since_filtering() {
        use crate::db;
        let pool = db::init_memory().unwrap();
        {
            let conn = pool.lock().unwrap();
            conn.execute("INSERT INTO exercises (id, name, exercise_type, created_at, updated_at) VALUES ('e1','Squat','weightlifting',1000,1000)", []).unwrap();
            conn.execute("INSERT INTO exercises (id, name, exercise_type, created_at, updated_at) VALUES ('e2','Bench','weightlifting',2000,2000)", []).unwrap();
            conn.execute("INSERT INTO exercises (id, name, exercise_type, created_at, updated_at, deleted_at) VALUES ('e3','Dead','weightlifting',3000,3000,3500)", []).unwrap();
        }
        let (port, handle) = crate::server::start_test_server_with_db(pool).await;
        let client = reqwest::Client::new();
        let base = format!("http://127.0.0.1:{port}");

        let resp = client
            .get(format!("{base}/exercises?since=0"))
            .header("Authorization", "Bearer test-key")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert!(body["server_time"].is_number());
        assert_eq!(body["items"].as_array().unwrap().len(), 2);
        assert_eq!(body["deleted"].as_array().unwrap().len(), 1);
        assert!(body["deleted"].as_array().unwrap().iter().any(|v| v=="e3"));

        let resp = client
            .get(format!("{base}/exercises?since=5000"))
            .header("Authorization", "Bearer test-key")
            .send()
            .await
            .unwrap();
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["items"].as_array().unwrap().len(), 0);
        assert_eq!(body["deleted"].as_array().unwrap().len(), 0);

        let resp = client.get(format!("{base}/exercises")).send().await.unwrap();
        assert_eq!(resp.status(), 401);

        handle.abort();
    }

    #[tokio::test]
    async fn pull_ingredients_nested() {
        use crate::db;
        let pool = db::init_memory().unwrap();
        {
            let conn = pool.lock().unwrap();
            conn.execute("INSERT INTO stores (id, name, created_at, updated_at) VALUES ('s1','Carrefour',1000,1000)", []).unwrap();
            conn.execute("INSERT INTO ingredients (id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, created_at, updated_at) VALUES ('i1','Oats',100,10,20,5,1000,1000)", []).unwrap();
            conn.execute("INSERT INTO ingredient_pictures (id, ingredient_id, image_path, sort_order, created_at, updated_at) VALUES ('p1','i1','/tmp/a.jpg',0,1000,1000)", []).unwrap();
            conn.execute("INSERT INTO ingredient_prices (id, ingredient_id, store_id, price, currency_code, recorded_at, created_at, updated_at) VALUES ('pr1','i1','s1',2.4,'EUR',1000,1000,1000)", []).unwrap();
        }
        let (port, handle) = crate::server::start_test_server_with_db(pool).await;
        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://127.0.0.1:{port}/ingredients?since=0"))
            .header("Authorization", "Bearer test-key")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["stores"].as_array().unwrap().len(), 1);
        let ing = &body["items"][0];
        assert_eq!(ing["pictures"].as_array().unwrap().len(), 1);
        assert_eq!(ing["prices"].as_array().unwrap().len(), 1);
        assert_eq!(ing["pictures"][0]["imagePath"], "/tmp/a.jpg");
        handle.abort();
    }

    #[tokio::test]
    async fn pull_fx_rates_filtered() {
        use crate::db;
        let pool = db::init_memory().unwrap();
        {
            let conn = pool.lock().unwrap();
            conn.execute("INSERT INTO fx_rates (code, base_code, rate_date, rate_to_base, updated_at) VALUES ('EUR','USD','2026-09-08',0.92,1000)", []).unwrap();
            conn.execute("INSERT INTO fx_rates (code, base_code, rate_date, rate_to_base, updated_at) VALUES ('GBP','USD','2026-09-08',0.78,2000)", []).unwrap();
            conn.execute("INSERT INTO fx_rates (code, base_code, rate_date, rate_to_base, updated_at) VALUES ('EUR','USD','2026-09-09',0.93,3000)", []).unwrap();
        }
        let (port, handle) = crate::server::start_test_server_with_db(pool).await;
        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://127.0.0.1:{port}/fx-rates?base=USD&date=2026-09-08&since=0"))
            .header("Authorization", "Bearer test-key")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["items"].as_array().unwrap().len(), 2);

        let resp = client
            .get(format!("http://127.0.0.1:{port}/fx-rates?base=USD&date=2026-09-08&since=1500"))
            .header("Authorization", "Bearer test-key")
            .send()
            .await
            .unwrap();
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["items"][0]["code"], "GBP");
        handle.abort();
    }

    #[tokio::test]
    async fn push_ingredient_idempotent() {
        use crate::db;
        let pool = db::init_memory().unwrap();
        let (port, handle) = crate::server::start_test_server_with_db(pool.clone()).await;
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{port}/ingredients");
        let payload = serde_json::json!({
            "id": "i1",
            "name": "Oats",
            "caloriesPer100g": 100.0,
            "proteinPer100g": 10.0,
            "carbsPer100g": 20.0,
            "fatPer100g": 5.0,
            "isArchived": false,
            "createdAt": 1000,
            "pictures": [{"id":"p1","ingredientId":"i1","imagePath":"/tmp/a.jpg","sortOrder":0,"createdAt":1000}],
            "prices": [{"id":"pr1","ingredientId":"i1","storeId":"s1","price":2.4,"currencyCode":"EUR","recordedAt":1000}]
        });
        let resp = client.post(&url).header("Authorization","Bearer test-key").json(&payload).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let resp2 = client.post(&url).header("Authorization","Bearer test-key").json(&payload).send().await.unwrap();
        assert_eq!(resp2.status(), 200);
        {
            let conn = pool.lock().unwrap();
            let count: i64 = conn.query_row("SELECT count(*) FROM ingredients WHERE id='i1'", [], |r| r.get(0)).unwrap();
            assert_eq!(count, 1);
            let count_p: i64 = conn.query_row("SELECT count(*) FROM ingredient_pictures WHERE id='p1'", [], |r| r.get(0)).unwrap();
            assert_eq!(count_p, 1);
        }
        handle.abort();
    }

    #[tokio::test]
    async fn push_workouts_atomic() {
        use crate::db;
        let pool = db::init_memory().unwrap();
        {
            let conn = pool.lock().unwrap();
            conn.execute("INSERT INTO exercises (id, name, exercise_type, created_at, updated_at) VALUES ('e1','Bench','weightlifting',1000,1000)", []).unwrap();
        }
        let (port, handle) = crate::server::start_test_server_with_db(pool.clone()).await;
        let client = reqwest::Client::new();
        let payload = serde_json::json!({
            "workouts": [{"id":"w1","name":"Push","date":1000,"createdAt":1000}],
            "workoutExercises": [{"id":"we1","workoutId":"w1","exerciseId":"e1","sortOrder":0}],
            "exerciseSets": [{"id":"s1","workoutExerciseId":"we1","setNumber":1,"reps":8,"weightKg":80.0}]
        });
        let resp = client.post(format!("http://127.0.0.1:{port}/workouts")).header("Authorization","Bearer test-key").json(&payload).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        {
            let conn = pool.lock().unwrap();
            let c: i64 = conn.query_row("SELECT count(*) FROM workouts WHERE id='w1'", [], |r| r.get(0)).unwrap();
            assert_eq!(c, 1);
            let c2: i64 = conn.query_row("SELECT count(*) FROM exercise_sets WHERE id='s1'", [], |r| r.get(0)).unwrap();
            assert_eq!(c2, 1);
        }
        handle.abort();
    }

    #[tokio::test]
    async fn push_templates_atomic() {
        use crate::db;
        let pool = db::init_memory().unwrap();
        {
            let conn = pool.lock().unwrap();
            conn.execute("INSERT INTO exercises (id, name, exercise_type, created_at, updated_at) VALUES ('e1','Bench','weightlifting',1000,1000)", []).unwrap();
        }
        let (port, handle) = crate::server::start_test_server_with_db(pool.clone()).await;
        let client = reqwest::Client::new();
        let payload = serde_json::json!({
            "workoutTemplates": [{"id":"t1","name":"MyTemplate","startDate":1000,"createdAt":1000,"updatedAt":1000}],
            "workoutTemplateExercises": [{"id":"te1","templateId":"t1","exerciseId":"e1","sortOrder":0}],
            "workoutTemplateSets": [{"id":"ts1","templateExerciseId":"te1","setNumber":1,"reps":8}]
        });
        let resp = client.post(format!("http://127.0.0.1:{port}/templates")).header("Authorization","Bearer test-key").json(&payload).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        {
            let conn = pool.lock().unwrap();
            let c: i64 = conn.query_row("SELECT count(*) FROM workout_templates WHERE id='t1'", [], |r| r.get(0)).unwrap();
            assert_eq!(c, 1);
        }
        handle.abort();
    }

    #[tokio::test]
    async fn receipt_pictures_json_fallback() {
        use crate::db;
        let pool = db::init_memory().unwrap();
        let (port, handle) = crate::server::start_test_server_with_db(pool.clone()).await;
        let client = reqwest::Client::new();
        let payload = serde_json::json!({"receipts":[{"id":"r1","localPath":"/tmp/a.jpg","uploadStatus":0,"parsed":false,"createdAt":1000}]});
        let resp = client.post(format!("http://127.0.0.1:{port}/receipt-pictures")).header("Authorization","Bearer test-key").json(&payload).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        {
            let conn = pool.lock().unwrap();
            let c: i64 = conn.query_row("SELECT count(*) FROM receipts WHERE id='r1'", [], |r| r.get(0)).unwrap();
            assert_eq!(c, 1);
        }
        handle.abort();
    }

    #[tokio::test]
    async fn backup_roundtrip() {
        use crate::db;
        let dir = std::env::temp_dir().join(format!("maia-test-backup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("fitfat_sync.db");
        let config = crate::config::Config::new(db_path.clone(), "test-key".to_string()).with_bind_addr("0.0.0.0:0");
        let (port, handle) = crate::server::start_server(config).await.unwrap();
        let client = reqwest::Client::new();
        let data = b"fake sqlite bytes".to_vec();
        let resp = client.post(format!("http://127.0.0.1:{port}/backup")).header("Authorization","Bearer test-key").header("Content-Type","application/octet-stream").body(data.clone()).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let resp = client.get(format!("http://127.0.0.1:{port}/backup/latest")).header("Authorization","Bearer test-key").send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let bytes = resp.bytes().await.unwrap();
        assert_eq!(bytes.to_vec(), data);
        handle.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn exercise_media_not_found_and_found() {
        let dir = std::env::temp_dir().join(format!("maia-test-media-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("exercise_media")).unwrap();
        std::fs::write(dir.join("exercise_media/test-id.jpg"), b"fake jpg").unwrap();
        let db_path = dir.join("fitfat_sync.db");
        let config = crate::config::Config::new(db_path, "test-key".to_string()).with_bind_addr("0.0.0.0:0");
        let (port, handle) = crate::server::start_server(config).await.unwrap();
        let client = reqwest::Client::new();
        let resp = client.get(format!("http://127.0.0.1:{port}/exercises/test-id.jpg")).header("Authorization","Bearer test-key").send().await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.headers().get("content-type").unwrap(), "image/jpeg");
        let resp2 = client.get(format!("http://127.0.0.1:{port}/media/test-id.jpg")).header("Authorization","Bearer test-key").send().await.unwrap();
        assert_eq!(resp2.status(), 200);
        assert_eq!(resp2.headers().get("content-type").unwrap(), "image/jpeg");
        let resp = client.get(format!("http://127.0.0.1:{port}/media/missing.jpg")).header("Authorization","Bearer test-key").send().await.unwrap();
        assert_eq!(resp.status(), 404);
        let resp = client.get(format!("http://127.0.0.1:{port}/media/test-id.jpg")).send().await.unwrap();
        assert_eq!(resp.status(), 401);
        handle.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn read_workouts_meals_body_metrics() {
        use crate::db;
        let pool = db::init_memory().unwrap();
        {
            let conn = pool.lock().unwrap();
            conn.execute("INSERT INTO workouts (id, name, date, created_at, updated_at) VALUES ('w1','Push',1000,1000,1000)", []).unwrap();
            conn.execute("INSERT INTO exercises (id, name, exercise_type, created_at, updated_at) VALUES ('e1','Bench','weightlifting',1000,1000)", []).unwrap();
            conn.execute("INSERT INTO workout_exercises (id, workout_id, exercise_id, sort_order, updated_at) VALUES ('we1','w1','e1',0,1000)", []).unwrap();
            conn.execute("INSERT INTO exercise_sets (id, workout_exercise_id, set_number, reps, weight_kg, updated_at) VALUES ('s1','we1',1,8,80,1000)", []).unwrap();
            conn.execute("INSERT INTO meals (id, name, eaten_at, created_at, updated_at) VALUES ('m1','Lunch',1000,1000,1000)", []).unwrap();
            conn.execute("INSERT INTO ingredients (id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, created_at, updated_at) VALUES ('i1','Oats',100,10,20,5,1000,1000)", []).unwrap();
            conn.execute("INSERT INTO meal_ingredients (id, meal_id, ingredient_id, grams, updated_at) VALUES ('mi1','m1','i1',100,1000)", []).unwrap();
            conn.execute("INSERT INTO body_metrics (id, date, weight_kg, created_at, updated_at) VALUES ('b1',1000,70.5,1000,1000)", []).unwrap();
        }
        let (port, handle) = crate::server::start_test_server_with_db(pool).await;
        let client = reqwest::Client::new();
        let resp = client.get(format!("http://127.0.0.1:{port}/workouts?since=0")).header("Authorization","Bearer test-key").send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["workouts"].as_array().unwrap().len(), 1);
        assert_eq!(body["workoutExercises"].as_array().unwrap().len(), 1);
        assert_eq!(body["exerciseSets"].as_array().unwrap().len(), 1);
        let resp = client.get(format!("http://127.0.0.1:{port}/meals?since=0")).header("Authorization","Bearer test-key").send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["meals"].as_array().unwrap().len(), 1);
        assert_eq!(body["mealIngredients"].as_array().unwrap().len(), 1);
        let resp = client.get(format!("http://127.0.0.1:{port}/body-metrics?since=0")).header("Authorization","Bearer test-key").send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["items"][0]["weightKg"], 70.5);
        handle.abort();
    }
}

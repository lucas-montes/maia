pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod handlers;
pub mod logs;
pub mod openapi;
pub mod openfoodfacts;
pub mod receipt_ocr;
pub mod seed;
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
            conn.execute("INSERT INTO ingredients (id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, created_at, updated_at) VALUES ('i2','Oats raw',400,13,60,7,1000,1000)", []).unwrap();
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
        assert_eq!(body["items"].as_array().unwrap().len(), 2);
        assert_eq!(body["stores"].as_array().unwrap().len(), 1);
        let ing = &body["items"][0];
        assert_eq!(ing["pictures"].as_array().unwrap().len(), 1);
        assert_eq!(ing["prices"].as_array().unwrap().len(), 1);
        assert_eq!(ing["pictures"][0]["imagePath"], "/tmp/a.jpg");
        // An ingredient is a leaf again: it carries its own nutrition and no
        // recipe. Composition lives on foods.
        assert!(ing.get("components").is_none());
        handle.abort();
    }

    #[tokio::test]
    async fn catalog_and_item_routes() {
        use crate::db;
        let pool = db::init_memory().unwrap();
        {
            let conn = pool.lock().unwrap();
            conn.execute("INSERT INTO exercises (id, name, exercise_type, created_at, updated_at) VALUES ('e1','Squat','weightlifting',1000,1000)", []).unwrap();
            conn.execute("INSERT INTO exercises (id, name, exercise_type, created_at, updated_at, deleted_at) VALUES ('e9','Gone','weightlifting',1000,1000,2000)", []).unwrap();
            conn.execute("INSERT INTO ingredients (id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, barcode, created_at, updated_at) VALUES ('i1','Oats',100,10,20,5,'123',1000,1000)", []).unwrap();
            conn.execute("INSERT INTO stores (id, name, created_at, updated_at) VALUES ('s1','Carrefour',1000,1000)", []).unwrap();
            conn.execute("INSERT INTO ingredient_pictures (id, ingredient_id, image_path, sort_order, created_at, updated_at) VALUES ('p1','i1','/tmp/a.jpg',0,1000,1000)", []).unwrap();
            conn.execute("INSERT INTO ingredient_prices (id, ingredient_id, store_id, price, currency_code, recorded_at, created_at, updated_at) VALUES ('pr1','i1','s1',2.4,'EUR',1000,1000,1000)", []).unwrap();
            conn.execute("INSERT INTO ingredients (id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, created_at, updated_at) VALUES ('i2','Oats raw',400,13,60,7,1000,1000)", []).unwrap();
        }
        let (port, handle) = crate::server::start_test_server_with_db(pool).await;
        let client = reqwest::Client::new();
        let base = format!("http://127.0.0.1:{port}");
        let auth = ("Authorization", "Bearer test-key");

        let resp = client.get(format!("{base}/exercises/catalog")).header(auth.0, auth.1).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        let items = body["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["id"], "e1");
        assert_eq!(items[0]["name"], "Squat");
        assert!(items[0].get("exerciseType").is_none());
        assert_eq!(items[0].get("has_image").and_then(|v| v.as_bool()), Some(false));

        let resp = client.get(format!("{base}/ingredients/catalog")).header(auth.0, auth.1).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        let items = body["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["barcode"], "123");
        assert!(items[0].get("pictures").is_none());
        assert!(items[0].get("prices").is_none());

        let resp = client.get(format!("{base}/exercises/item/e1")).header(auth.0, auth.1).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["name"], "Squat");
        assert!(body.get("hasImage").is_some());

        let resp = client.get(format!("{base}/exercises/item/e9")).header(auth.0, auth.1).send().await.unwrap();
        assert_eq!(resp.status(), 404);
        let resp = client.get(format!("{base}/exercises/item/missing")).header(auth.0, auth.1).send().await.unwrap();
        assert_eq!(resp.status(), 404);

        let resp = client.get(format!("{base}/ingredients/item/i1")).header(auth.0, auth.1).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["barcode"], "123");
        assert_eq!(body["pictures"].as_array().unwrap().len(), 1);
        assert!(body.get("prices").is_none());
        assert!(body.get("stores").is_none());
        // Selective import carries nutrition only. Composition belongs to foods,
        // so it is not part of an ingredient payload at all.
        assert!(body.get("components").is_none());

        let resp = client.get(format!("{base}/ingredients/item/missing")).header(auth.0, auth.1).send().await.unwrap();
        assert_eq!(resp.status(), 404);

        let resp = client.get(format!("{base}/exercises/catalog")).send().await.unwrap();
        assert_eq!(resp.status(), 401);

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
    async fn lookup_local_hit_and_invalid() {
        use crate::db;
        let pool = db::init_memory().unwrap();
        {
            let conn = pool.lock().unwrap();
            conn.execute("INSERT INTO ingredients (id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, barcode, created_at, updated_at) VALUES ('i1','Oats',100,10,20,5,'3017620422003',1000,1000)", []).unwrap();
        }
        let (port, handle) = crate::server::start_test_server_with_db(pool).await;
        let client = reqwest::Client::new();
        let base = format!("http://127.0.0.1:{port}");

        let resp = client
            .get(format!("{base}/ingredients/lookup?barcode=3017620422003"))
            .header("Authorization", "Bearer test-key")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["source"], "local");
        assert_eq!(body["ingredient"]["barcode"], "3017620422003");
        assert_eq!(
            body["openfoodUrl"],
            "https://world.openfoodfacts.org/product/3017620422003"
        );

        let resp = client
            .get(format!("{base}/ingredients/lookup?barcode=abc"))
            .header("Authorization", "Bearer test-key")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400);

        let resp = client
            .get(format!("{base}/ingredients/lookup?barcode=3017620422003"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 401);

        handle.abort();
    }

    #[tokio::test]
    async fn push_ingredient_idempotent() {
        use crate::db;
        let pool = db::init_memory().unwrap();
        {
            let conn = pool.lock().unwrap();
            conn.execute("INSERT INTO stores (id, name, created_at, updated_at) VALUES ('s1','Carrefour',1000,1000)", []).unwrap();
        }
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
    async fn push_foods_idempotent_and_replaces_composition() {
        use crate::db;
        let pool = db::init_memory().unwrap();
        {
            let conn = pool.lock().unwrap();
            // Composition parts must already exist: food_ingredients has a real
            // FK and push unwraps, so an unknown part is skipped rather than
            // aborting the whole request.
            conn.execute("INSERT INTO ingredients (id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, created_at, updated_at) VALUES ('i2','Oats raw',400,13,60,7,1000,1000)", []).unwrap();
            conn.execute("INSERT INTO ingredients (id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, created_at, updated_at) VALUES ('i3','Honey',300,0,80,0,1000,1000)", []).unwrap();
        }
        let (port, handle) = crate::server::start_test_server_with_db(pool.clone()).await;
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{port}/foods");
        let payload = serde_json::json!({
            "items": [{
                "id": "f1",
                "name": "Granola",
                "createdAt": 1000,
                "archivedAt": serde_json::Value::Null,
                "components": [
                    {"ingredientId": "i2", "amount": 80.0},
                    {"ingredientId": "i3", "amount": 20.0}
                ]
            }]
        });
        let resp = client.post(&url).header("Authorization","Bearer test-key").json(&payload).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        // Re-pushing must not duplicate the food or its composition.
        let resp2 = client.post(&url).header("Authorization","Bearer test-key").json(&payload).send().await.unwrap();
        assert_eq!(resp2.status(), 200);
        {
            let conn = pool.lock().unwrap();
            let count: i64 = conn.query_row("SELECT count(*) FROM foods WHERE id='f1'", [], |r| r.get(0)).unwrap();
            assert_eq!(count, 1);
            let count_c: i64 = conn.query_row("SELECT count(*) FROM food_ingredients WHERE food_id='f1'", [], |r| r.get(0)).unwrap();
            assert_eq!(count_c, 2);
            // The same ingredient twice in one food is impossible by schema,
            // not merely rejected by the server.
            let dup: i64 = conn.query_row("SELECT count(*) FROM (SELECT ingredient_id FROM food_ingredients WHERE food_id='f1' GROUP BY ingredient_id HAVING count(*)>1)", [], |r| r.get(0)).unwrap();
            assert_eq!(dup, 0);
        }

        // Replace semantics: pushing a shorter composition drops the missing
        // part instead of accumulating a union of both versions.
        let trimmed = serde_json::json!({
            "items": [{
                "id": "f1", "name": "Granola", "createdAt": 1000,
                "components": [{"ingredientId": "i2", "amount": 150.0}]
            }]
        });
        let resp = client.post(&url).header("Authorization","Bearer test-key").json(&trimmed).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        {
            let conn = pool.lock().unwrap();
            let rows: Vec<(String, f64)> = conn
                .prepare("SELECT ingredient_id, amount FROM food_ingredients WHERE food_id='f1' AND deleted_at IS NULL")
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(rows, vec![("i2".to_string(), 150.0)]);
        }

        // A part whose ingredient this server has never seen is skipped without
        // failing the whole push — and it leaves the food with no composition
        // rather than a partial one.
        let unknown = serde_json::json!({
            "items": [{
                "id": "f1", "name": "Granola", "createdAt": 1000,
                "components": [{"ingredientId": "never-pushed", "amount": 10.0}]
            }]
        });
        let resp = client.post(&url).header("Authorization","Bearer test-key").json(&unknown).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        {
            let conn = pool.lock().unwrap();
            let count_c: i64 = conn.query_row("SELECT count(*) FROM food_ingredients WHERE food_id='f1' AND deleted_at IS NULL", [], |r| r.get(0)).unwrap();
            assert_eq!(count_c, 0);
        }
        handle.abort();
    }

    #[tokio::test]
    async fn pull_foods_nests_composition_without_nutrition() {
        use crate::db;
        let pool = db::init_memory().unwrap();
        {
            let conn = pool.lock().unwrap();
            conn.execute("INSERT INTO ingredients (id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, created_at, updated_at) VALUES ('i2','Oats raw',400,13,60,7,1000,1000)", []).unwrap();
            conn.execute("INSERT INTO ingredients (id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, created_at, updated_at) VALUES ('i3','Honey',300,0,80,0,1000,1000)", []).unwrap();
            conn.execute("INSERT INTO foods (id, name, created_at, updated_at) VALUES ('f1','Granola',1000,1000)", []).unwrap();
            conn.execute("INSERT INTO food_ingredients (food_id, ingredient_id, amount, updated_at) VALUES ('f1','i3',20,1000)", []).unwrap();
            conn.execute("INSERT INTO food_ingredients (food_id, ingredient_id, amount, updated_at) VALUES ('f1','i2',80,1000)", []).unwrap();
        }
        let (port, handle) = crate::server::start_test_server_with_db(pool).await;
        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://127.0.0.1:{port}/foods?since=0"))
            .header("Authorization", "Bearer test-key")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        let items = body["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["name"], "Granola");
        // Components come back nested, ordered by ingredient name.
        let components = items[0]["components"].as_array().unwrap();
        assert_eq!(components.len(), 2);
        assert_eq!(components[0]["ingredientId"], "i3"); // Honey
        assert_eq!(components[0]["amount"], 20.0);
        assert_eq!(components[1]["ingredientId"], "i2"); // Oats raw
        assert_eq!(components[1]["amount"], 80.0);
        // A food carries no nutrition of its own: it is always resolved from the
        // ingredient rows, so the server must not store or send a stale copy.
        assert!(items[0].get("caloriesPer100g").is_none());
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
        assert_eq!(resp.status(), 405);
        {
            let conn = pool.lock().unwrap();
            let c: i64 = conn.query_row("SELECT count(*) FROM workouts WHERE id='w1'", [], |r| r.get(0)).unwrap();
            assert_eq!(c, 0);
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
        assert_eq!(resp.status(), 404);
        let resp = client.get(format!("http://127.0.0.1:{port}/backup/latest")).header("Authorization","Bearer test-key").send().await.unwrap();
        assert_eq!(resp.status(), 404);
        handle.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // Receipt OCR pipeline
    //
    // These exercise the request/response and persistence contract. The model
    // call itself is not mocked: `receipt_ocr_live_gemini` covers it and only
    // runs when `MAIA_TEST_GEMINI_KEY` is set.
    // -----------------------------------------------------------------------

    /// Spins up a server on a temp DB with a (fake) model key so `state.ocr` is
    /// populated, and returns the temp dir so the caller can clean up.
    async fn start_receipt_ocr_server(model_key: Option<&str>) -> (u16, tokio::task::JoinHandle<()>, std::path::PathBuf) {
        use crate::db;
        let dir = std::env::temp_dir().join(format!("maia-test-ocr-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("maia.db");
        let mut config = crate::config::Config::new(db_path, "test-key".to_string()).with_bind_addr("0.0.0.0:0");
        if let Some(k) = model_key {
            config = config.with_model(k, "gemini-2.5-flash");
        }
        let pool = db::init_db(&config.db_path).unwrap();
        let (port, handle) = crate::server::start_test_server_with_config(config, pool).await;
        (port, handle, dir)
    }

    fn ocr_form(id: &str, bytes: &[u8], mime: &str) -> reqwest::multipart::Form {
        let part = reqwest::multipart::Part::bytes(bytes.to_vec())
            .file_name(format!("{id}.jpg"))
            .mime_str(mime)
            .unwrap();
        reqwest::multipart::Form::new()
            .text("receiptId", id.to_string())
            .part("picture", part)
    }

    #[tokio::test]
    async fn receipt_ocr_upload_requires_auth() {
        let (port, handle, dir) = start_receipt_ocr_server(Some("fake-key")).await;
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("http://127.0.0.1:{port}/receipt-pictures/upload"))
            .multipart(ocr_form("r-auth", b"\xff\xd8\xff", "image/jpeg"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 401);
        handle.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn receipt_ocr_without_model_key_is_503() {
        let (port, handle, dir) = start_receipt_ocr_server(None).await;
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("http://127.0.0.1:{port}/receipt-pictures/upload"))
            .header("Authorization", "Bearer test-key")
            .multipart(ocr_form("r-nokey", b"\xff\xd8\xff", "image/jpeg"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 503);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert!(
            body["message"].as_str().unwrap().contains("ocr not configured"),
            "unexpected body {body}"
        );
        handle.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn receipt_ocr_upload_persists_image_and_row() {
        let (port, handle, dir) = start_receipt_ocr_server(Some("fake-key")).await;
        let client = reqwest::Client::new();
        // A PNG payload, so the stored extension follows the declared MIME.
        let png = b"\x89PNG\r\n\x1a\nfake".to_vec();
        let resp = client
            .post(format!("http://127.0.0.1:{port}/receipt-pictures/upload"))
            .header("Authorization", "Bearer test-key")
            .multipart(ocr_form("r-persist", &png, "image/png"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 202);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["id"], "r-persist");
        assert_eq!(body["status"], "parsing");

        // Image is on disk under the DB directory, and the row points at it
        // with a *relative* path so no server layout leaks to clients.
        let stored = dir.join("receipt_pictures").join("r-persist.png");
        assert!(stored.exists(), "expected {} to exist", stored.display());
        assert_eq!(std::fs::read(&stored).unwrap(), png);

        let pool = crate::db::init_db(&dir.join("maia.db")).unwrap();
        let conn = pool.lock().unwrap();
        let row: (String, i64, i64) = conn
            .query_row(
                "SELECT remote_path, upload_status, parsed FROM receipts WHERE id = 'r-persist'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(row.0, "receipt_pictures/r-persist.png");
        assert_eq!(row.1, 4, "expected upload_status parsing(4)");
        assert_eq!(row.2, 0, "parsed must start false");
        drop(conn);
        handle.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn receipt_ocr_parse_and_picture_endpoints() {
        let (port, handle, dir) = start_receipt_ocr_server(Some("fake-key")).await;
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("http://127.0.0.1:{port}/receipt-pictures/upload"))
            .header("Authorization", "Bearer test-key")
            .multipart(ocr_form("r-poll", b"\xff\xd8\xff", "image/jpeg"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 202);

        // The image is served back byte-for-byte.
        let resp = client
            .get(format!("http://127.0.0.1:{port}/receipt-pictures/r-poll/picture"))
            .header("Authorization", "Bearer test-key")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers()["content-type"].to_str().unwrap(),
            "image/jpeg"
        );
        assert_eq!(resp.bytes().await.unwrap().as_ref(), b"\xff\xd8\xff");

        // The parse eventually reaches a terminal state. The fake key means the
        // upstream call fails, which lands in `error` — the point is that the
        // row always stops being `parsing`.
        let mut status = String::new();
        for _ in 0..60 {
            let body: serde_json::Value = client
                .get(format!("http://127.0.0.1:{port}/receipt-pictures/r-poll/parse"))
                .header("Authorization", "Bearer test-key")
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            status = body["status"].as_str().unwrap().to_string();
            if status == "error" || status == "parsed" {
                assert_eq!(body["id"], "r-poll");
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        assert!(
            status == "error" || status == "parsed",
            "parse never left `parsing`, last status {status}"
        );

        // Unknown ids 404 rather than erroring.
        let resp = client
            .get(format!("http://127.0.0.1:{port}/receipt-pictures/nope/parse"))
            .header("Authorization", "Bearer test-key")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 404);
        handle.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn receipt_ocr_rejects_bad_input() {
        let (port, handle, dir) = start_receipt_ocr_server(Some("fake-key")).await;
        let client = reqwest::Client::new();

        // No `picture` part at all.
        let form = reqwest::multipart::Form::new().text("receiptId", "r-empty".to_string());
        let resp = client
            .post(format!("http://127.0.0.1:{port}/receipt-pictures/upload"))
            .header("Authorization", "Bearer test-key")
            .multipart(form)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400);

        // An empty picture part.
        let form = reqwest::multipart::Form::new()
            .text("receiptId", "r-empty2".to_string())
            .part(
                "picture",
                reqwest::multipart::Part::bytes(Vec::new())
                    .file_name("r-empty2.jpg")
                    .mime_str("image/jpeg")
                    .unwrap(),
            );
        let resp = client
            .post(format!("http://127.0.0.1:{port}/receipt-pictures/upload"))
            .header("Authorization", "Bearer test-key")
            .multipart(form)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400);

        // A `receiptId` that would escape the picture directory.
        let resp = client
            .post(format!("http://127.0.0.1:{port}/receipt-pictures/upload"))
            .header("Authorization", "Bearer test-key")
            .multipart(ocr_form("../../escape", b"\xff\xd8\xff", "image/jpeg"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400);

        // And the traversal target was not created.
        assert!(!dir.parent().unwrap().join("escape.jpg").exists());
        handle.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn receipt_push_preserves_archive_state() {
        // A phone push must not wipe archive bookkeeping recorded by the
        // desktop: the legacy handler's ON CONFLICT clause lists columns
        // explicitly, so archived_* are left alone.
        use crate::db;
        let pool = db::init_memory().unwrap();
        {
            let conn = pool.lock().unwrap();
            conn.execute(
                "INSERT INTO receipts (id, local_path, upload_status, parsed, created_at, archived_path, archived_at)
                 VALUES ('r-arch', '/tmp/a.jpg', 2, 1, 1000, 'receipts_archive/r-arch', 5000)",
                [],
            )
            .unwrap();
        }
        let (port, handle) = crate::server::start_test_server_with_db(pool.clone()).await;
        let client = reqwest::Client::new();
        let payload = serde_json::json!({"receipts":[{"id":"r-arch","localPath":"/tmp/a.jpg","remotePath":"receipt_pictures/r-arch.jpg","uploadStatus":2,"parsed":true,"createdAt":1000}]});
        let resp = client
            .post(format!("http://127.0.0.1:{port}/receipt-pictures"))
            .header("Authorization", "Bearer test-key")
            .json(&payload)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let conn = pool.lock().unwrap();
        let (arch_path, arch_at): (Option<String>, Option<i64>) = conn
            .query_row(
                "SELECT archived_path, archived_at FROM receipts WHERE id='r-arch'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(arch_path.as_deref(), Some("receipts_archive/r-arch"));
        assert_eq!(arch_at, Some(5000));
        drop(conn);
        handle.abort();
    }

    #[tokio::test]
    async fn receipt_push_stores_relative_remote_path() {
        // `remote_path` reaches the phone via `pull_receipts`, so it must stay
        // relative rather than leaking the server's absolute directory.
        let dir = std::env::temp_dir().join(format!("maia-test-rel-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("maia.db");
        let config = crate::config::Config::new(db_path.clone(), "test-key".to_string())
            .with_bind_addr("0.0.0.0:0");
        let pool = crate::db::init_db(&config.db_path).unwrap();
        let (port, handle) = crate::server::start_test_server_with_config(config, pool.clone()).await;

        // Exercise the multipart branch: `receipt` JSON field + `picture` bytes.
        let boundary = "----maiaTestBoundary";
        let body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"receipt\"\r\n\r\n\
             {}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"picture\"; filename=\"r-rel.jpg\"\r\nContent-Type: image/jpeg\r\n\r\n{}\r\n--{boundary}--\r\n",
            serde_json::json!({
                "id": "r-rel",
                "localPath": "/phone/path/r-rel.jpg",
                "uploadStatus": 2,
                "parsed": false,
                "createdAt": 1000
            }),
            "JPEGDATA"
        );
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("http://127.0.0.1:{port}/receipt-pictures"))
            .header("Authorization", "Bearer test-key")
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(body)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        let conn = pool.lock().unwrap();
        let remote: Option<String> = conn
            .query_row(
                "SELECT remote_path FROM receipts WHERE id='r-rel'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        drop(conn);

        let remote = remote.expect("remote_path should be set by the multipart branch");
        assert_eq!(remote, "receipt_pictures/r-rel.jpg");
        assert!(
            !std::path::Path::new(&remote).is_absolute(),
            "remote_path must not be absolute: {remote}"
        );
        assert!(!remote.contains(std::env::temp_dir().to_string_lossy().as_ref()));
        assert!(dir.join("receipt_pictures").join("r-rel.jpg").exists());

        handle.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Live domain-logic check for the OCR pipeline, calling the vision model
    /// directly. Deliberately excludes the DB and the axum endpoints: this
    /// covers only the conversion chain (image bytes → base64 → Gemini → JSON
    /// text → `ParsedReceipt`), so a failure points at the model, the prompt or
    /// the schema rather than at request plumbing.
    ///
    /// Ignored by default because it needs a real key and costs one API call:
    /// ```sh
    /// GEMINI_API_KEY=... cargo test -p sync-server receipt_ocr_live -- --ignored --nocapture
    /// ```
    /// Requires a receipt image in `sync-server/testdata/` — see
    /// `sync-server/testdata/README.md`. That directory is gitignored.
    #[tokio::test]
    #[ignore = "needs a real Gemini key and a receipt image; run with --ignored --nocapture"]
    async fn receipt_ocr_live_gemini() {
        /// Hardcoded input, relative to this crate; change here for a different
        /// filename. Anchored to the manifest because `cargo test` runs with the
        /// crate directory as CWD, not the workspace root.
        const RECEIPT_IMAGE: &str = "testdata/receipt.jpg";
        const MODEL: &str = "gemini-2.5-flash";

        let key = ["GEMINI_API_KEY", "MAIA_TEST_GEMINI_KEY"]
            .into_iter()
            .find_map(|name| std::env::var(name).ok().filter(|v| !v.trim().is_empty()))
            .unwrap_or_else(|| {
                panic!("set GEMINI_API_KEY (or MAIA_TEST_GEMINI_KEY) to run the live OCR test")
            });

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(RECEIPT_IMAGE);
        assert!(
            path.exists(),
            "receipt image not found at {} — see sync-server/testdata/README.md",
            path.display()
        );
        let bytes = std::fs::read(&path).expect("read receipt image");
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_else(|| "jpg".to_string());
        let mime = crate::handlers::media::content_type_for_ext(&ext);
        println!("input: {} ({} bytes, {mime})", path.display(), bytes.len());

        use ai::Vllm;
        use crate::receipt_ocr::ParsedReceipt;
        let model = ai::Model::with_model(key, MODEL.to_string());

        // The whole conversion: bytes → base64 → Gemini → JSON text → struct.
        let parsed = match model.extract_image_bytes::<ParsedReceipt>(bytes, mime).await {
            Ok(Some(p)) => p,
            Ok(None) => panic!("model returned no content for {}", path.display()),
            Err(e) => panic!("model call failed for {}: {e:?}", path.display()),
        };

        // Fields the mobile draft builder reads out of this map.
        assert!(
            !parsed.merchant.trim().is_empty(),
            "merchant is empty; got {parsed:?}"
        );
        assert!(
            !parsed.currency.trim().is_empty(),
            "currency is empty; got {parsed:?}"
        );
        assert!(parsed.total > 0.0, "total is not positive; got {parsed:?}");
        assert!(
            !parsed.date.trim().is_empty(),
            "date is empty; got {parsed:?}"
        );

        // Round-trip through JSON: these are the exact keys the client depends
        // on, and the pre-rename names must stay gone.
        let json = serde_json::to_value(&parsed).expect("serialise parsed receipt");
        assert!(json.get("merchant").is_some(), "missing `merchant`: {json}");
        assert!(json.get("items").is_some(), "missing `items`: {json}");
        assert!(json.get("store").is_none(), "legacy `store` leaked: {json}");
        assert!(
            json.get("products").is_none(),
            "legacy `products` leaked: {json}"
        );

        println!("parsed {} line item(s)", parsed.items.len());
        println!(
            "{}",
            serde_json::to_string_pretty(&json).expect("pretty-print parsed receipt")
        );
    }

    // -----------------------------------------------------------------------
    // Domain-logic conversion chain, against a stub Gemini.
    //
    // These two are NOT ignored: they run in the default suite and need no API
    // key or network. They cover the same conversion the live test does —
    // bytes → base64 → request JSON → response JSON text → `ParsedReceipt` —
    // plus the rejected-key path, by pointing `ai::Model::with_base_url` at a
    // local server that imitates Gemini's envelopes.
    // -----------------------------------------------------------------------

    /// Starts a stub Gemini and returns its base URL plus a handle to abort.
    async fn stub_gemini(
        status: axum::http::StatusCode,
        body: serde_json::Value,
    ) -> (String, tokio::task::JoinHandle<()>) {
        async fn handler(
            axum::extract::State(state): axum::extract::State<(
                axum::http::StatusCode,
                serde_json::Value,
            )>,
        ) -> impl axum::response::IntoResponse {
            let (status, body) = state;
            (status, axum::Json(body))
        }
        // The client appends `/{model}:generateContent` to the base URL, so match on
        // a fallback rather than hard-coding that path.
        let app = axum::Router::new()
            .fallback(axum::routing::post(handler))
            .with_state((status, body));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://127.0.0.1:{port}"), handle)
    }

    #[tokio::test]
    async fn receipt_ocr_converts_gemini_response_into_parsed_receipt() {
        use ai::Vllm;
        use crate::receipt_ocr::ParsedReceipt;

        // The JSON the model is expected to emit, embedded as the text part.
        let inner = serde_json::json!({
            "merchant": "Carrefour",
            "date": "2026-08-15 14:30:00",
            "total": 42.75,
            "currency": "EUR",
            "items": [{"name": "Milk", "price": 1.2, "currency": "EUR"}],
            "discounts": [{"description": "loyalty", "amount": 2.0}]
        });
        let (base_url, handle) = stub_gemini(
            axum::http::StatusCode::OK,
            serde_json::json!({
                "candidates": [{
                    "content": {"role": "model", "parts": [{"text": inner.to_string()}]},
                    "finishReason": "STOP"
                }],
                "modelVersion": "gemini-2.5-flash"
            }),
        )
        .await;

        let model = ai::Model::with_base_url("test-key".into(), "gemini-2.5-flash".into(), base_url);
        let parsed = model
            .extract_image_bytes::<ParsedReceipt>(b"\xff\xd8\xffnot-a-real-jpeg".to_vec(), "image/jpeg")
            .await
            .expect("call stub gemini")
            .expect("stub returned content");

        // Domain object is populated from the model's JSON text.
        assert_eq!(parsed.merchant, "Carrefour");
        assert_eq!(parsed.date, "2026-08-15 14:30:00");
        assert_eq!(parsed.total, 42.75);
        assert_eq!(parsed.currency, "EUR");
        assert_eq!(parsed.items.len(), 1);
        assert_eq!(parsed.items[0].name, "Milk");
        assert_eq!(parsed.discounts.len(), 1);
        assert_eq!(parsed.discounts[0].amount, 2.0);

        // And the keys the mobile draft builder reads are present, with the
        // pre-rename names absent.
        let json = serde_json::to_value(&parsed).unwrap();
        assert_eq!(json["merchant"], "Carrefour");
        assert!(json["items"].is_array());
        assert!(json.get("store").is_none());
        assert!(json.get("products").is_none());
        handle.abort();
    }

    #[tokio::test]
    async fn receipt_ocr_surfaces_gemini_rejection_instead_of_decode_error() {
        use ai::Vllm;
        use crate::receipt_ocr::ParsedReceipt;

        // Gemini answers a bad key with a non-2xx and an `error.message`
        // envelope. Decoding that as a success used to surface as
        // "missing field `candidates`", hiding the real cause.
        let (base_url, handle) = stub_gemini(
            axum::http::StatusCode::BAD_REQUEST,
            serde_json::json!({"error": {"code": 400, "message": "API key not valid. Please pass a valid API key.", "status": "INVALID_ARGUMENT"}}),
        )
        .await;

        let model = ai::Model::with_base_url("bad-key".into(), "gemini-2.5-flash".into(), base_url);
        let err = model
            .extract_image_bytes::<ParsedReceipt>(b"\xff\xd8\xff".to_vec(), "image/jpeg")
            .await
            .expect_err("a 400 must not be reported as success");

        match err {
            ai::ModelError::Api { status, ref message } => {
                assert_eq!(status, 400);
                assert!(
                    message.contains("API key not valid"),
                    "unexpected message: {message}"
                );
            }
            other => panic!("expected ModelError::Api, got {other:?}"),
        }
        handle.abort();
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
        // Exercise media is public so plain <img>/<video> tags (no
        // Authorization header) can load it; JSON routes stay protected.
        let resp = client.get(format!("http://127.0.0.1:{port}/media/test-id.jpg")).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.headers().get("content-type").unwrap(), "image/jpeg");
        let resp = client.get(format!("http://127.0.0.1:{port}/exercises/test-id.jpg")).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let resp = client.get(format!("http://127.0.0.1:{port}/exercises?since=0")).send().await.unwrap();
        assert_eq!(resp.status(), 401);
        let resp = client.post(format!("http://127.0.0.1:{port}/ingredients")).send().await.unwrap();
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
            conn.execute("INSERT INTO foods (id, name, created_at, updated_at) VALUES ('f1','Oats bowl',1000,1000)", []).unwrap();
            conn.execute("INSERT INTO food_ingredients (food_id, ingredient_id, amount, updated_at) VALUES ('f1','i1',100,1000)", []).unwrap();
            // A portion snapshot, frozen at log time.
            conn.execute("INSERT INTO meal_foods (id, meal_id, food_id, amount, calories, protein, carbs, fat, sodium, fiber, sugar, updated_at) VALUES ('mf1','m1','f1',200,200,20,40,10,NULL,NULL,NULL,1000)", []).unwrap();
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
        let meal_foods = body["mealFoods"].as_array().unwrap();
        assert_eq!(meal_foods.len(), 1);
        // The frozen portion is replayed verbatim, and the food name is joined
        // live so a rename shows up in old meals without moving the numbers.
        assert_eq!(meal_foods[0]["foodId"], "f1");
        assert_eq!(meal_foods[0]["foodName"], "Oats bowl");
        assert_eq!(meal_foods[0]["amount"], 200.0);
        assert_eq!(meal_foods[0]["calories"], 200.0);
        let resp = client.get(format!("http://127.0.0.1:{port}/body-metrics?since=0")).header("Authorization","Bearer test-key").send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["items"][0]["weightKg"], 70.5);
        handle.abort();
    }
}

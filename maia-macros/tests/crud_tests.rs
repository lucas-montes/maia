use maia_macros::Crud;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

// Test struct with basic fields
#[derive(Crud, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[table_name = "products"]
struct Product {
    #[skip_crud]
    id: Option<i64>,
    name: String,
    calories: f64,
    protein: f64,
}

// Test struct with optional fields
#[derive(Crud, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[table_name = "users"]
struct User {
    #[skip_crud]
    id: Option<i64>,
    username: String,
    email: Option<String>,
    age: Option<i32>,
}

// Test struct with custom primary key
#[derive(Crud, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[table_name = "items"]
#[primary_key = "item_id"]
struct Item {
    #[skip_crud]
    item_id: Option<i64>,
    name: String,
    quantity: i32,
}

fn setup_product_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute(
        "CREATE TABLE products (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            calories REAL NOT NULL,
            protein REAL NOT NULL
        )",
        [],
    )
    .unwrap();
    conn
}

fn setup_user_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute(
        "CREATE TABLE users (
            id INTEGER PRIMARY KEY,
            username TEXT NOT NULL,
            email TEXT,
            age INTEGER
        )",
        [],
    )
    .unwrap();
    conn
}

fn setup_item_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute(
        "CREATE TABLE items (
            item_id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            quantity INTEGER NOT NULL
        )",
        [],
    )
    .unwrap();
    conn
}

#[test]
fn test_create_product() {
    let conn = setup_product_db();

    let product = Product {
        id: None,
        name: "Chicken Breast".to_string(),
        calories: 165.0,
        protein: 31.0,
    };

    let id = Product::create(&conn, &product).unwrap();
    assert!(id > 0);
}

#[test]
fn test_find_product() {
    let conn = setup_product_db();

    let product = Product {
        id: None,
        name: "Salmon".to_string(),
        calories: 206.0,
        protein: 22.0,
    };

    let id = Product::create(&conn, &product).unwrap();

    let found = Product::find(&conn, id).unwrap();
    assert!(found.is_some());

    let found_product = found.unwrap();
    assert_eq!(found_product.name, "Salmon");
    assert_eq!(found_product.calories, 206.0);
    assert_eq!(found_product.protein, 22.0);
    assert_eq!(found_product.id, Some(id));
}

#[test]
fn test_find_nonexistent_product() {
    let conn = setup_product_db();

    let found = Product::find(&conn, 999).unwrap();
    assert!(found.is_none());
}

#[test]
fn test_update_product() {
    let conn = setup_product_db();

    let product = Product {
        id: None,
        name: "Eggs".to_string(),
        calories: 155.0,
        protein: 13.0,
    };

    let id = Product::create(&conn, &product).unwrap();

    let mut updated_product = Product::find(&conn, id).unwrap().unwrap();
    updated_product.name = "Free Range Eggs".to_string();
    updated_product.calories = 160.0;

    Product::update(&updated_product, &conn).unwrap();

    let found = Product::find(&conn, id).unwrap().unwrap();
    assert_eq!(found.name, "Free Range Eggs");
    assert_eq!(found.calories, 160.0);
    assert_eq!(found.protein, 13.0); // Should remain unchanged
}

#[test]
fn test_delete_product() {
    let conn = setup_product_db();

    let product = Product {
        id: None,
        name: "Milk".to_string(),
        calories: 42.0,
        protein: 3.4,
    };

    let id = Product::create(&conn, &product).unwrap();

    // Verify it exists
    assert!(Product::find(&conn, id).unwrap().is_some());

    // Delete it
    Product::delete(&conn, id).unwrap();

    // Verify it's gone
    assert!(Product::find(&conn, id).unwrap().is_none());
}

#[test]
fn test_list_products() {
    let conn = setup_product_db();

    let products = vec![
        Product {
            id: None,
            name: "Apple".to_string(),
            calories: 52.0,
            protein: 0.3,
        },
        Product {
            id: None,
            name: "Banana".to_string(),
            calories: 89.0,
            protein: 1.1,
        },
        Product {
            id: None,
            name: "Orange".to_string(),
            calories: 47.0,
            protein: 0.9,
        },
    ];

    for product in &products {
        Product::create(&conn, product).unwrap();
    }

    let all_products = Product::list(&conn).unwrap();
    assert_eq!(all_products.len(), 3);

    assert_eq!(all_products[0].name, "Apple");
    assert_eq!(all_products[1].name, "Banana");
    assert_eq!(all_products[2].name, "Orange");
}

#[test]
fn test_list_empty() {
    let conn = setup_product_db();

    let all_products = Product::list(&conn).unwrap();
    assert_eq!(all_products.len(), 0);
}

#[test]
fn test_table_name() {
    assert_eq!(Product::table_name(), "products");
    assert_eq!(User::table_name(), "users");
    assert_eq!(Item::table_name(), "items");
}

#[test]
fn test_optional_fields() {
    let conn = setup_user_db();

    // User with all fields
    let user1 = User {
        id: None,
        username: "alice".to_string(),
        email: Some("alice@example.com".to_string()),
        age: Some(30),
    };

    let id1 = User::create(&conn, &user1).unwrap();
    let found1 = User::find(&conn, id1).unwrap().unwrap();
    assert_eq!(found1.email, Some("alice@example.com".to_string()));
    assert_eq!(found1.age, Some(30));

    // User with optional fields as None
    let user2 = User {
        id: None,
        username: "bob".to_string(),
        email: None,
        age: None,
    };

    let id2 = User::create(&conn, &user2).unwrap();
    let found2 = User::find(&conn, id2).unwrap().unwrap();
    assert_eq!(found2.email, None);
    assert_eq!(found2.age, None);
}

#[test]
fn test_custom_primary_key() {
    let conn = setup_item_db();

    let item = Item {
        item_id: None,
        name: "Widget".to_string(),
        quantity: 100,
    };

    let id = Item::create(&conn, &item).unwrap();

    let found = Item::find(&conn, id).unwrap().unwrap();
    assert_eq!(found.name, "Widget");
    assert_eq!(found.quantity, 100);
    assert_eq!(found.item_id, Some(id));

    // Update
    let mut updated = found.clone();
    updated.quantity = 150;
    Item::update(&updated, &conn).unwrap();

    let found_updated = Item::find(&conn, id).unwrap().unwrap();
    assert_eq!(found_updated.quantity, 150);

    // Delete
    Item::delete(&conn, id).unwrap();
    assert!(Item::find(&conn, id).unwrap().is_none());
}

#[test]
fn test_multiple_operations() {
    let conn = setup_product_db();

    // Create
    let product = Product {
        id: None,
        name: "Rice".to_string(),
        calories: 130.0,
        protein: 2.7,
    };
    let id = Product::create(&conn, &product).unwrap();

    // Find
    let mut found = Product::find(&conn, id).unwrap().unwrap();
    assert_eq!(found.name, "Rice");

    // Update
    found.name = "Brown Rice".to_string();
    found.calories = 112.0;
    Product::update(&found, &conn).unwrap();

    // Verify update
    let updated = Product::find(&conn, id).unwrap().unwrap();
    assert_eq!(updated.name, "Brown Rice");
    assert_eq!(updated.calories, 112.0);

    // List
    let all = Product::list(&conn).unwrap();
    assert_eq!(all.len(), 1);

    // Delete
    Product::delete(&conn, id).unwrap();

    // Verify deletion
    assert!(Product::find(&conn, id).unwrap().is_none());
    let all_after_delete = Product::list(&conn).unwrap();
    assert_eq!(all_after_delete.len(), 0);
}

#[test]
fn test_concurrent_inserts() {
    let conn = setup_product_db();

    for i in 0..10 {
        let product = Product {
            id: None,
            name: format!("Product {}", i),
            calories: (100 + i) as f64,
            protein: (10 + i) as f64,
        };
        Product::create(&conn, &product).unwrap();
    }

    let all = Product::list(&conn).unwrap();
    assert_eq!(all.len(), 10);

    // Verify IDs are sequential
    for (i, product) in all.iter().enumerate() {
        assert_eq!(product.id, Some((i + 1) as i64));
        assert_eq!(product.name, format!("Product {}", i));
    }
}

#[test]
#[should_panic(expected = "InvalidQuery")]
fn test_update_without_id() {
    let conn = setup_product_db();

    let product = Product {
        id: None, // No ID!
        name: "Should Fail".to_string(),
        calories: 100.0,
        protein: 10.0,
    };

    // This should panic because id is None
    Product::update(&product, &conn).unwrap();
}

#[test]
fn test_update_nonexistent_record() {
    let conn = setup_product_db();

    let product = Product {
        id: Some(999), // Non-existent ID
        name: "Ghost Product".to_string(),
        calories: 100.0,
        protein: 10.0,
    };

    // Update should succeed but affect 0 rows
    let result = Product::update(&product, &conn);
    assert!(result.is_ok());

    // Verify it wasn't actually created
    let found = Product::find(&conn, 999).unwrap();
    assert!(found.is_none());
}

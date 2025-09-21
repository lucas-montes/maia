use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FoodItem {
    id: Option<i64>,
    name: String,
    calories_per_100g: f64,
    protein: Option<f64>,
    carbs: Option<f64>,
    fat: Option<f64>,
    fiber: Option<f64>,
    created_at: Option<String>,
}

impl FoodItem {
    pub fn new(name: String, calories_per_100g: f64, protein: Option<f64>, carbs: Option<f64>, fat: Option<f64>, fiber: Option<f64>, created_at: Option<String>) -> Self {
        Self {
            id: None,
            name,
            calories_per_100g,
            protein,
            carbs,
            fat,
            fiber,
            created_at,
        }
    }
    pub fn id(&self) -> Option<i64> { self.id }
    pub fn calories_per_100g(&self) -> f64 { self.calories_per_100g }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FoodLog {
    id: Option<i64>,
    food_item_id: i64,
    food_item_name: String,
    quantity: f64, // in grams
    meal_type: String, // breakfast, lunch, dinner, snack
    date: String,
    calories_consumed: f64,
    image_path: Option<PathBuf>,
    created_at: Option<String>,
}

impl FoodLog {
    pub fn new(food_item_id: i64, food_item_name: String, quantity: f64, meal_type: String, date: String, calories_consumed: f64, image_path: Option<PathBuf>, created_at: Option<String>) -> Self {
        Self {
            id: None,
            food_item_id,
            food_item_name,
            quantity,
            meal_type,
            date,
            calories_consumed,
            image_path,
            created_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WeightLog {
    id: Option<i64>,
    weight: f64, // in kg
    date: String,
    notes: Option<String>,
    created_at: Option<String>,
}

impl WeightLog {
    pub fn new(weight: f64, date: String, notes: Option<String>, created_at: Option<String>) -> Self {
        Self {
            id: None,
            weight,
            date,
            notes,
            created_at,
        }
    }
    pub fn weight(&self) -> f64 { self.weight }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkoutLog {
    id: Option<i64>,
    exercise: String,
    weight: Option<f64>, // in kg
    reps: Option<u32>,
    sets: Option<u32>,
    duration_minutes: Option<u32>,
    date: String,
    notes: Option<String>,
    created_at: Option<String>,
}

impl WorkoutLog {
    pub fn new(exercise: String, weight: Option<f64>, reps: Option<u32>, sets: Option<u32>, duration_minutes: Option<u32>, date: String, notes: Option<String>, created_at: Option<String>) -> Self {
        Self {
            id: None,
            exercise,
            weight,
            reps,
            sets,
            duration_minutes,
            date,
            notes,
            created_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GroceryPurchase {
    id: Option<i64>,
    store: Option<String>,
    total_amount: Option<f64>,
    date: String,
    receipt_image_path: Option<PathBuf>,
    expense_id: Option<i64>, // Reference to finance domain
    items: Vec<GroceryItem>,
    created_at: Option<String>,
}

impl GroceryPurchase {
    pub fn new(store: Option<String>, total_amount: Option<f64>, date: String, receipt_image_path: Option<PathBuf>, expense_id: Option<i64>, items: Vec<GroceryItem>, created_at: Option<String>) -> Self {
        Self {
            id: None,
            store,
            total_amount,
            date,
            receipt_image_path,
            expense_id,
            items,
            created_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GroceryItem {
    id: Option<i64>,
    purchase_id: i64,
    food_item_id: Option<i64>, // Link to food items
    name: String,
    quantity: f64,
    unit: String, // kg, items, etc.
    price: Option<f64>,
}

impl GroceryItem {
    pub fn new(purchase_id: i64, food_item_id: Option<i64>, name: String, quantity: f64, unit: String, price: Option<f64>) -> Self {
        Self {
            id: None,
            purchase_id,
            food_item_id,
            name,
            quantity,
            unit,
            price,
        }
    }
}

// Summary structures for reports
#[derive(Debug, Serialize, Deserialize)]
pub struct DailyNutritionSummary {
    date: String,
    total_calories: f64,
    total_protein: f64,
    total_carbs: f64,
    total_fat: f64,
    meals: Vec<FoodLog>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WeightTrend {
    entries: Vec<WeightLog>,
    start_weight: f64,
    current_weight: f64,
    change: f64,
    trend: String, // "increasing", "decreasing", "stable"
}

impl WeightTrend {
    pub fn new(entries: Vec<WeightLog>, start_weight: f64, current_weight: f64, change: f64, trend: String) -> Self {
        Self {
            entries,
            start_weight,
            current_weight,
            change,
            trend,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkoutProgress {
    exercise: String,
    entries: Vec<WorkoutLog>,
    max_weight: Option<f64>,
    max_reps: Option<u32>,
    total_sessions: usize,
}

use std::path::PathBuf;
use crate::models::*;
use crate::cli::{AddFoodItem, LogFood, LogWeight, LogWorkout};

pub trait HealthDb: Send + Sync {
    fn insert_food_item(&self, food: &FoodItem) -> Result<i64, String>;
    fn get_food_item_by_name(&self, name: &str) -> Result<FoodItem, String>;
    fn insert_food_log(&self, log: &FoodLog) -> Result<i64, String>;
    fn insert_weight_log(&self, log: &WeightLog) -> Result<i64, String>;
    fn get_weight_logs_last_n_days(&self, days: u32) -> Result<Vec<WeightLog>, String>;
    fn insert_workout_log(&self, log: &WorkoutLog) -> Result<i64, String>;
    fn insert_grocery_purchase(&self, purchase: &GroceryPurchase) -> Result<i64, String>;
    fn list_food_logs(&self, date: Option<String>) -> Result<Vec<FoodLog>, String>;
    fn list_groceries(&self) -> Result<Vec<GroceryPurchase>, String>;
    fn list_workout_progress(&self, exercise: String) -> Result<Vec<WorkoutLog>, String>;
}

pub fn add_food_item(db: &impl HealthDb, food_item: &AddFoodItem) -> Result<i64, String> {
    let food = FoodItem::new(
        food_item.name().to_string(),
        food_item.calories_per_100g(),
        food_item.protein(),
        food_item.carbs(),
        food_item.fat(),
        food_item.fiber(),
        Some(chrono::Utc::now().to_rfc3339()),
    );
    db.insert_food_item(&food)
}

pub fn log_food(db: &impl HealthDb, food_log: &LogFood) -> Result<i64, String> {
    let food_item = db.get_food_item_by_name(food_log.food_item())?;
    let calories_per_100g = food_item.calories_per_100g();
    let id = food_item.id().unwrap();
    let calories_consumed = (calories_per_100g * food_log.quantity()) / 100.0;
    let log = FoodLog::new(
        id,
        food_log.food_item().to_string(),
        food_log.quantity(),
        food_log.meal_type().cloned().unwrap_or_else(|| "meal".to_string()),
        food_log.date().cloned().unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string()),
        calories_consumed,
        food_log.image().cloned(),
        Some(chrono::Utc::now().to_rfc3339()),
    );
    db.insert_food_log(&log)
}

pub fn log_weight(db: &impl HealthDb, weight_log: &LogWeight) -> Result<i64, String> {
    let log = WeightLog::new(
        weight_log.weight(),
        weight_log.date().cloned().unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string()),
        weight_log.notes().cloned(),
        Some(chrono::Utc::now().to_rfc3339()),
    );
    db.insert_weight_log(&log)
}

pub fn get_weight_trend(db: &impl HealthDb, days: u32) -> Result<WeightTrend, String> {
    let entries = db.get_weight_logs_last_n_days(days)?;
    if entries.is_empty() {
        return Ok(WeightTrend::new(
            entries,
            0.0,
            0.0,
            0.0,
            "no_data".to_string(),
        ));
    }
    let start_weight = entries.first().unwrap().weight();
    let current_weight = entries.last().unwrap().weight();
    let change = current_weight - start_weight;
    let trend = if change > 1.0 {
        "increasing".to_string()
    } else if change < -1.0 {
        "decreasing".to_string()
    } else {
        "stable".to_string()
    };
    Ok(WeightTrend::new(
        entries,
        start_weight,
        current_weight,
        change,
        trend,
    ))
}

pub fn log_workout(db: &impl HealthDb, workout_log: &LogWorkout) -> Result<i64, String> {
    let log = WorkoutLog::new(
        workout_log.exercise().to_string(),
        workout_log.weight(),
        workout_log.reps(),
        workout_log.sets(),
        workout_log.duration_minutes(),
        workout_log.date().cloned().unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string()),
        workout_log.notes().cloned(),
        Some(chrono::Utc::now().to_rfc3339()),
    );
    db.insert_workout_log(&log)
}

pub fn process_grocery_receipt(_receipt_image: PathBuf, _store: Option<String>, _date: String) -> Result<ReceiptData, String> {
    // TODO: Integrate with AI service to process receipt
    // For now, return mock data
    Ok(ReceiptData {
        total_amount: 45.67,
        items: vec![
            GroceryItem::new(0, None, "Bananas".to_string(), 1.2, "kg".to_string(), Some(2.40)),
            GroceryItem::new(0, None, "Chicken Breast".to_string(), 0.8, "kg".to_string(), Some(8.99)),
        ],
    })
}

pub fn list_food_logs(db: &impl HealthDb, date: Option<String>) -> Result<Vec<FoodLog>, String> {
    db.list_food_logs(date)
}

pub fn list_groceries(db: &impl HealthDb) -> Result<Vec<GroceryPurchase>, String> {
    db.list_groceries()
}

pub fn list_workout_progress(db: &impl HealthDb, exercise: String) -> Result<Vec<WorkoutLog>, String> {
    db.list_workout_progress(exercise)
}

#[derive(Debug, Clone)]
pub struct ReceiptData {
    pub total_amount: f64,
    pub items: Vec<GroceryItem>,
}

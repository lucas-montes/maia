use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// Cross-domain command that involves both health and finance
#[derive(Debug, Serialize, Deserialize)]
pub enum CrossDomainCommand {
    // Grocery purchase that creates both health and finance records
    ProcessGroceryReceipt {
        receipt_image: PathBuf,
        store: Option<String>,
        date: String,
    },

    // Get nutrition summary with expense correlation
    GetNutritionWithCosts {
        start_date: String,
        end_date: String,
    },
}

// Domain orchestration events
#[derive(Debug, Serialize, Deserialize)]
pub enum DomainEvent {
    // Health events
    FoodItemCreated { food_item_id: i64, name: String },
    FoodLogged { food_log_id: i64, food_item_id: i64, calories: f64, date: String },
    WeightLogged { weight_log_id: i64, weight: f64, date: String },
    WorkoutLogged { workout_id: i64, exercise: String, date: String },

    // Finance events (from finance crate)
    ExpenseCreated { expense_id: i64, amount: f64, category: String, date: String },

    // Cross-domain events
    GroceryPurchaseProcessed {
        purchase_id: i64,
        expense_id: i64,
        total_amount: f64,
        items: Vec<String>,
        date: String
    },
}

// Response types for cross-domain operations
#[derive(Debug, Serialize, Deserialize)]
pub enum CrossDomainResponse {
    GroceryProcessed {
        purchase_id: i64,
        expense_id: i64,
        items_added: u32,
        total_amount: f64,
    },
    NutritionWithCosts {
        total_calories: f64,
        total_cost: f64,
        cost_per_calorie: f64,
        daily_summaries: Vec<DailySummary>,
    },
    Error(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DailySummary {
    pub date: String,
    pub calories: f64,
    pub cost: f64,
    pub meals: u32,
}

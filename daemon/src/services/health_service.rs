use health::service::HealthDb;
use crate::database::Database;

impl HealthDb for Database {
    fn insert_food_item(&self, food: &health::models::FoodItem) -> Result<i64, String> {
        self.insert_food_item(food)
    }
    fn get_food_item_by_name(&self, name: &str) -> Result<health::models::FoodItem, String> {
        self.get_food_item_by_name(name)
    }
    fn insert_food_log(&self, log: &health::models::FoodLog) -> Result<i64, String> {
        self.insert_food_log(log)
    }
    fn insert_weight_log(&self, log: &health::models::WeightLog) -> Result<i64, String> {
        self.insert_weight_log(log)
    }
    fn get_weight_logs_last_n_days(&self, days: u32) -> Result<Vec<health::models::WeightLog>, String> {
        self.get_weight_logs_last_n_days(days)
    }
    fn insert_workout_log(&self, log: &health::models::WorkoutLog) -> Result<i64, String> {
        self.insert_workout_log(log)
    }
    fn insert_grocery_purchase(&self, purchase: &health::models::GroceryPurchase) -> Result<i64, String> {
        self.insert_grocery_purchase(purchase)
    }
    fn list_food_logs(&self, date: Option<String>) -> Result<Vec<health::models::FoodLog>, String> {
        self.list_food_logs(date)
    }
    fn list_groceries(&self) -> Result<Vec<health::models::GroceryPurchase>, String> {
        self.list_groceries()
    }
    fn list_workout_progress(&self, exercise: String) -> Result<Vec<health::models::WorkoutLog>, String> {
        self.list_workout_progress(exercise)
    }
}

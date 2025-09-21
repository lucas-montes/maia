use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use shared::protocol::MessageProtocol;

/// Health CLI
#[cfg_attr(feature = "cli", derive(clap::Args))]
#[derive(Debug, Serialize, Deserialize)]
pub struct Cli {
    /// Health command group
    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    fn message(self) -> Vec<u8> {
        self.command.to_bytes().expect("msgage serialization failed")
    }
}

/// Health commands
#[cfg_attr(feature = "cli", derive(clap::Subcommand))]
#[derive(Debug, Serialize, Deserialize)]
enum Commands {
    /// Log food consumption
    #[cfg_attr(feature = "cli", command(name = "log-food"))]
    LogFood(LogFood),
    /// Add a new food item
    #[cfg_attr(feature = "cli", command(name = "add-food-item"))]
    AddFoodItem(AddFoodItem),
    /// List food logs
    #[cfg_attr(feature = "cli", command(name = "list-food"))]
    ListFood {
        /// Date to filter (YYYY-MM-DD)
        #[cfg_attr(feature = "cli", arg(short, long))]
        date: Option<String>
    },

    /// Log weight
    #[cfg_attr(feature = "cli", command(name = "log-weight"))]
    LogWeight(LogWeight),
    /// Show weight trend
    #[cfg_attr(feature = "cli", command(name = "weight-trend"))]
    WeightTrend {
        /// Number of days to show
        #[cfg_attr(feature = "cli", arg(short, long, default_value = "30"))]
        days: u32
    },

    /// Log a workout
    #[cfg_attr(feature = "cli", command(name = "log-workout"))]
    LogWorkout(LogWorkout),
    /// Show workout progress
    #[cfg_attr(feature = "cli", command(name = "workout-progress"))]
    WorkoutProgress {
        /// Exercise name
        #[cfg_attr(feature = "cli", arg(short, long))]
        exercise: String
    },

    /// Add groceries from a receipt
    #[cfg_attr(feature = "cli", command(name = "add-groceries"))]
    AddGroceries(AddGroceries),
    /// List grocery purchases
    #[cfg_attr(feature = "cli", command(name = "list-groceries"))]
    ListGroceries,
}

impl MessageProtocol for Commands {}

/// Log food consumption
#[cfg_attr(feature = "cli", derive(clap::Args))]
#[derive(Debug, Serialize, Deserialize)]
pub struct LogFood {
    /// Food item name
    #[cfg_attr(feature = "cli", arg(short, long))]
    food_item: String,
    /// Quantity in grams
    #[cfg_attr(feature = "cli", arg(short, long))]
    quantity: f64,
    /// Meal type (breakfast, lunch, etc)
    #[cfg_attr(feature = "cli", arg(short, long))]
    meal_type: Option<String>,
    /// Date (YYYY-MM-DD)
    #[cfg_attr(feature = "cli", arg(short, long))]
    date: Option<String>,
    /// Image of the meal
    #[cfg_attr(feature = "cli", arg(long))]
    image: Option<PathBuf>,
}

impl LogFood {
    pub fn food_item(&self) -> &str { &self.food_item }
    pub fn quantity(&self) -> f64 { self.quantity }
    pub fn meal_type(&self) -> Option<&String> { self.meal_type.as_ref() }
    pub fn date(&self) -> Option<&String> { self.date.as_ref() }
    pub fn image(&self) -> Option<&PathBuf> { self.image.as_ref() }
}

/// Add a new food item
#[cfg_attr(feature = "cli", derive(clap::Args))]
#[derive(Debug, Serialize, Deserialize)]
pub struct AddFoodItem {
    /// Name of the food
    #[cfg_attr(feature = "cli", arg(short, long))]
    name: String,
    /// Calories per 100g
    #[cfg_attr(feature = "cli", arg(short, long))]
    calories_per_100g: f64,
    /// Protein per 100g
    #[cfg_attr(feature = "cli", arg(short, long))]
    protein: Option<f64>,
    /// Carbs per 100g
    #[cfg_attr(feature = "cli", arg(long))]
    carbs: Option<f64>,
    /// Fat per 100g
    #[cfg_attr(feature = "cli", arg(long))]
    fat: Option<f64>,
    /// Fiber per 100g
    #[cfg_attr(feature = "cli", arg(long))]
    fiber: Option<f64>,
}

impl AddFoodItem {
    pub fn name(&self) -> &str { &self.name }
    pub fn calories_per_100g(&self) -> f64 { self.calories_per_100g }
    pub fn protein(&self) -> Option<f64> { self.protein }
    pub fn carbs(&self) -> Option<f64> { self.carbs }
    pub fn fat(&self) -> Option<f64> { self.fat }
    pub fn fiber(&self) -> Option<f64> { self.fiber }
}

/// Log weight
#[cfg_attr(feature = "cli", derive(clap::Args))]
#[derive(Debug, Serialize, Deserialize)]
pub struct LogWeight {
    /// Weight in kg
    #[cfg_attr(feature = "cli", arg(short, long))]
    weight: f64,
    /// Date (YYYY-MM-DD)
    #[cfg_attr(feature = "cli", arg(short, long))]
    date: Option<String>,
    /// Notes
    #[cfg_attr(feature = "cli", arg(short, long))]
    notes: Option<String>,
}

impl LogWeight {
    pub fn weight(&self) -> f64 { self.weight }
    pub fn date(&self) -> Option<&String> { self.date.as_ref() }
    pub fn notes(&self) -> Option<&String> { self.notes.as_ref() }
}

/// Log a workout
#[cfg_attr(feature = "cli", derive(clap::Args))]
#[derive(Debug, Serialize, Deserialize)]
pub struct LogWorkout {
    /// Exercise name
    #[cfg_attr(feature = "cli", arg(short, long))]
    exercise: String,
    /// Weight used (kg)
    #[cfg_attr(feature = "cli", arg(short, long))]
    weight: Option<f64>,
    /// Repetitions
    #[cfg_attr(feature = "cli", arg(short, long))]
    reps: Option<u32>,
    /// Sets
    #[cfg_attr(feature = "cli", arg(short, long))]
    sets: Option<u32>,
    /// Duration in minutes
    #[cfg_attr(feature = "cli", arg(short, long))]
    duration_minutes: Option<u32>,
    /// Date (YYYY-MM-DD)
    #[cfg_attr(feature = "cli", arg(short, long))]
    date: Option<String>,
    /// Notes
    #[cfg_attr(feature = "cli", arg(short, long))]
    notes: Option<String>,
}

impl LogWorkout {
    pub fn exercise(&self) -> &str { &self.exercise }
    pub fn weight(&self) -> Option<f64> { self.weight }
    pub fn reps(&self) -> Option<u32> { self.reps }
    pub fn sets(&self) -> Option<u32> { self.sets }
    pub fn duration_minutes(&self) -> Option<u32> { self.duration_minutes }
    pub fn date(&self) -> Option<&String> { self.date.as_ref() }
    pub fn notes(&self) -> Option<&String> { self.notes.as_ref() }
}

/// Add groceries from a receipt
#[cfg_attr(feature = "cli", derive(clap::Args))]
#[derive(Debug, Serialize, Deserialize)]
pub struct AddGroceries {
    /// Path to the receipt image
    #[cfg_attr(feature = "cli", arg(short, long))]
    receipt_image: Option<PathBuf>,
    /// Store name
    #[cfg_attr(feature = "cli", arg(short, long))]
    store: Option<String>,
    /// Total amount
    #[cfg_attr(feature = "cli", arg(short, long))]
    total_amount: Option<f64>,
    /// Date (YYYY-MM-DD)
    #[cfg_attr(feature = "cli", arg(short, long))]
    date: Option<String>,
}

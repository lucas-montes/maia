use serde::{Deserialize, Serialize};

/// Main command enum that encompasses all possible CLI commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    Todo(TodoCommand),
    Notes(NotesCommand),
    Expenses(ExpensesCommand),
    Calories(CaloriesCommand),
}

/// Todo-related commands matching the existing todo crate structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TodoCommand {
    Add {
        title: String,
        description: Option<String>,
    },
    List {
        status: Option<String>, // "pending" | "done"
    },
    Complete {
        id: u32,
    },
    Delete {
        id: u32,
    },
}

/// Notes-related commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotesCommand {
    Add {
        title: String,
        content: String,
    },
    List {
        search: Option<String>,
    },
    Edit {
        id: u32,
        content: String,
    },
    Delete {
        id: u32,
    },
}

/// Expenses-related commands with AI integration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExpensesCommand {
    Add {
        image_paths: Vec<String>,
    },
    List {
        month: Option<String>, // Format: "2024-01"
    },
}

/// Calorie tracking commands with AI integration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CaloriesCommand {
    Add {
        food: String,
        quantity: f64,
        image_path: Option<String>,
    },
    List {
        date: Option<String>, // Format: "2024-01-15"
    },
    Update {
        id: u32,
        quantity: f64,
    },
    Delete {
        id: u32,
    },
}

/// Response from daemon to CLI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    Success(String),
    Error(String),
    Data(serde_json::Value),
}

/// Database models for todos
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id: u32,
    pub title: String,
    pub description: Option<String>,
    pub status: String, // "pending" | "done"
    pub created_at: String,
    pub updated_at: Option<String>,
}

/// Database models for notes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: u32,
    pub title: String,
    pub file_path: String, // Path to the actual note file
    pub created_at: String,
    pub updated_at: Option<String>,
}

/// Database models for expenses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expense {
    pub id: u32,
    pub store: Option<String>,
    pub total: f64,
    pub date: String,
    pub raw_data: serde_json::Value, // Full AI response
    pub created_at: String,
}

/// Individual expense items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpenseItem {
    pub id: u32,
    pub expense_id: u32,
    pub name: String,
    pub price: f64,
    pub quantity: f64,
    pub category: Option<String>,
}

/// Database models for calorie entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalorieEntry {
    pub id: u32,
    pub food_item: String,
    pub quantity: f64,
    pub unit: String,
    pub calories_total: f64,
    pub date: String,
    pub expense_item_id: Option<u32>, // Link to purchases
    pub created_at: String,
}

/// AI schema for expense parsing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpenseData {
    pub store: Option<String>,
    pub total: f64,
    pub date: String,
    pub items: Vec<ExpenseItemData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpenseItemData {
    pub name: String,
    pub price: f64,
    pub quantity: f64,
    pub category: String,
}

/// AI schema for calorie parsing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalorieData {
    pub food_item: String,
    pub calories_per_unit: f64,
    pub unit: String,
    pub serving_size: Option<String>,
    pub macros: Option<MacroData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroData {
    pub protein: f64,
    pub carbs: f64,
    pub fat: f64,
    pub fiber: Option<f64>,
}

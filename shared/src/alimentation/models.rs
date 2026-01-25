// Database models for the health module

use serde::{Deserialize, Serialize};
use chrono::NaiveDateTime;
use maia_macros::Crud;


/// Database model representing an aliment and the nutritional values
#[derive(Debug, Serialize, Deserialize, Crud)]
#[table_name = "aliments"]
pub struct Aliment {
    id: Option<i64>,
    name: String,
    calories: f32,
    proteins: f32,
    carbs: f32,
    fats: f32,
    fiber: f64,
    sugar: f64,
    sodium: f64,
    saturated_fat: f64,
    source: String,
    source_type: String,
    brand: String,
    barcode: String,
}


#[derive(Debug, Serialize, Deserialize, Crud)]
#[table_name = "meals"]
pub struct Meal {
    id: Option<i64>,
    date: NaiveDateTime,
    aliment_id: Option<i64>,
    quantity: f32,
}

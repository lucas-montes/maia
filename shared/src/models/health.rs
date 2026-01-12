use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aliment {
    id: Option<i64>,
    name: String,
    brand: Option<String>,
    barcode: Option<String>,

    // Nutritional values per 100g/100ml
    calories: f64,           // kcal
    protein: f64,            // grams
    carbohydrates: f64,      // grams
    fat: f64,                // grams
    fiber: Option<f64>,      // grams
    sugar: Option<f64>,      // grams
    sodium: Option<f64>,     // mg
    saturated_fat: Option<f64>, // grams

    // Source information
    source_type: Option<String>, // "supermarket", "manual", "api"
    source_url: Option<String>,   // URL to product page or nutritional info
    source_image: Option<String>, // Path to saved image of nutritional info

    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodyMetric {
    id: Option<i64>,
    date: NaiveDate,
    weight: Option<f64>,
    body_fat_percentage: Option<f64>,
    muscle_mass: Option<f64>,
    notes: Option<String>,
    created_at: DateTime<Utc>,
}

impl BodyMetric {
    pub fn new(date: NaiveDate) -> Self {
        Self {
            id: None,
            date,
            weight: None,
            body_fat_percentage: None,
            muscle_mass: None,
            notes: None,
            created_at: Utc::now(),
        }
    }

    pub fn with_id(mut self, id: i64) -> Self {
        self.id = Some(id);
        self
    }

    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = Some(weight);
        self
    }

    pub fn with_body_fat_percentage(mut self, body_fat_percentage: f64) -> Self {
        self.body_fat_percentage = Some(body_fat_percentage);
        self
    }

    pub fn with_muscle_mass(mut self, muscle_mass: f64) -> Self {
        self.muscle_mass = Some(muscle_mass);
        self
    }

    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }

    pub fn with_created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.created_at = created_at;
        self
    }

    // Getters
    pub fn id(&self) -> Option<i64> { self.id }
    pub fn date(&self) -> NaiveDate { self.date }
    pub fn weight(&self) -> Option<f64> { self.weight }
    pub fn body_fat_percentage(&self) -> Option<f64> { self.body_fat_percentage }
    pub fn muscle_mass(&self) -> Option<f64> { self.muscle_mass }
    pub fn notes(&self) -> Option<&str> { self.notes.as_deref() }
    pub fn created_at(&self) -> DateTime<Utc> { self.created_at }
}

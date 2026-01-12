use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::Crud;

/// Product with full CRUD operations via derive macro
#[derive(Debug, Clone, Serialize, Deserialize, Crud)]
#[table_name = "products"]
pub struct Product {
    #[skip_crud]
    pub id: Option<i64>,

    pub name: String,
    pub brand: Option<String>,
    pub barcode: Option<String>,

    // Nutritional values per 100g/100ml
    pub calories: f64,
    pub protein: f64,
    pub carbohydrates: f64,
    pub fat: f64,
    pub fiber: Option<f64>,
    pub sugar: Option<f64>,
    pub sodium: Option<f64>,
    pub saturated_fat: Option<f64>,

    // Source information
    pub source_type: Option<String>,
    pub source_url: Option<String>,
    pub source_image: Option<String>,

    #[skip_crud]
    pub created_at: DateTime<Utc>,
    #[skip_crud]
    pub updated_at: DateTime<Utc>,
}

impl Product {
    /// Create a new Product instance (not yet in database)
    pub fn new(
        name: String,
        calories: f64,
        protein: f64,
        carbohydrates: f64,
        fat: f64,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            name,
            brand: None,
            barcode: None,
            calories,
            protein,
            carbohydrates,
            fat,
            fiber: None,
            sugar: None,
            sodium: None,
            saturated_fat: None,
            source_type: None,
            source_url: None,
            source_image: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_brand(mut self, brand: impl Into<String>) -> Self {
        self.brand = Some(brand.into());
        self
    }

    pub fn with_barcode(mut self, barcode: impl Into<String>) -> Self {
        self.barcode = Some(barcode.into());
        self
    }

    pub fn with_source(
        mut self,
        source_type: impl Into<String>,
        source_url: Option<String>,
        source_image: Option<String>,
    ) -> Self {
        self.source_type = Some(source_type.into());
        self.source_url = source_url;
        self.source_image = source_image;
        self
    }
}

// The Crud macro automatically generates:
// - Product::create(conn, &product) -> Result<i64>
// - Product::find(conn, id) -> Result<Option<Product>>
// - product.update(conn) -> Result<()>
// - Product::delete(conn, id) -> Result<()>
// - Product::list(conn) -> Result<Vec<Product>>
// - Product::table_name() -> &'static str

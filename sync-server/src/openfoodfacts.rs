use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngredientDraft {
    pub name: String,
    pub brand: Option<String>,
    pub barcode: String,
    #[serde(rename = "caloriesPer100g")]
    pub calories_per100g: f64,
    #[serde(rename = "proteinPer100g")]
    pub protein_per100g: f64,
    #[serde(rename = "carbsPer100g")]
    pub carbs_per100g: f64,
    #[serde(rename = "fatPer100g")]
    pub fat_per100g: f64,
    #[serde(rename = "sodiumPer100g")]
    pub sodium_per100g: Option<f64>,
    #[serde(rename = "fiberPer100g")]
    pub fiber_per100g: Option<f64>,
    #[serde(rename = "sugarPer100g")]
    pub sugar_per100g: Option<f64>,
}

pub fn openfood_url(barcode: &str) -> String {
    format!("https://world.openfoodfacts.org/product/{barcode}")
}

pub fn valid_barcode(barcode: &str) -> bool {
    !barcode.is_empty()
        && barcode.len() <= 14
        && barcode.bytes().all(|b| b.is_ascii_digit())
        && (8..=14).contains(&barcode.len())
}

#[derive(Debug, Deserialize)]
struct OffResponse {
    status: i64,
    product: Option<OffProduct>,
}

#[derive(Debug, Deserialize)]
struct OffProduct {
    #[serde(default)]
    product_name: Option<String>,
    #[serde(default)]
    brands: Option<String>,
    #[serde(default)]
    nutriments: Option<OffNutriments>,
}

#[derive(Debug, Deserialize)]
struct OffNutriments {
    #[serde(default, deserialize_with = "de_opt_f64")]
    #[serde(rename = "energy-kcal_100g")]
    energy_kcal_100g: Option<f64>,
    #[serde(default, deserialize_with = "de_opt_f64")]
    proteins_100g: Option<f64>,
    #[serde(default, deserialize_with = "de_opt_f64")]
    carbohydrates_100g: Option<f64>,
    #[serde(default, deserialize_with = "de_opt_f64")]
    fat_100g: Option<f64>,
    #[serde(default, deserialize_with = "de_opt_f64")]
    sodium_100g: Option<f64>,
    #[serde(default, deserialize_with = "de_opt_f64")]
    fiber_100g: Option<f64>,
    #[serde(default, deserialize_with = "de_opt_f64")]
    sugars_100g: Option<f64>,
}

fn de_opt_f64<'de, D>(d: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Option<f64>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("number or numeric string")
        }
        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D2: serde::Deserializer<'de>>(
            self,
            d: D2,
        ) -> Result<Self::Value, D2::Error> {
            d.deserialize_any(V)
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
            Ok(Some(v))
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
            Ok(Some(v as f64))
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
            Ok(Some(v as f64))
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            v.trim().parse::<f64>().map(Some).map_err(|_| {
                E::custom(format!("invalid number: {v}"))
            })
        }
        fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
            self.visit_str(&v)
        }
    }
    d.deserialize_option(V)
}

pub fn draft_from_off(barcode: &str, body: &serde_json::Value) -> Option<IngredientDraft> {
    let resp: OffResponse = serde_json::from_value(body.clone()).ok()?;
    if resp.status != 1 {
        return None;
    }
    let p = resp.product?;
    let name = p.product_name.filter(|s| !s.trim().is_empty())?;
    let nutr = p.nutriments.unwrap_or(OffNutriments {
        energy_kcal_100g: None,
        proteins_100g: None,
        carbohydrates_100g: None,
        fat_100g: None,
        sodium_100g: None,
        fiber_100g: None,
        sugars_100g: None,
    });
    // Sodium in OFF is grams/100g; our schema stores mg/100g.
    let sodium_mg = nutr.sodium_100g.map(|g| g * 1000.0);
    Some(IngredientDraft {
        name: name.trim().to_string(),
        brand: p.brands.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        barcode: barcode.to_string(),
        calories_per100g: nutr.energy_kcal_100g.unwrap_or(0.0),
        protein_per100g: nutr.proteins_100g.unwrap_or(0.0),
        carbs_per100g: nutr.carbohydrates_100g.unwrap_or(0.0),
        fat_per100g: nutr.fat_100g.unwrap_or(0.0),
        sodium_per100g: sodium_mg,
        fiber_per100g: nutr.fiber_100g,
        sugar_per100g: nutr.sugars_100g,
    })
}

pub async fn fetch_product(barcode: &str) -> anyhow::Result<Option<IngredientDraft>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .user_agent("MaiaFitFat/1.0")
        .build()?;
    let url = format!("https://world.openfoodfacts.org/api/v2/product/{barcode}.json");
    let resp = client.get(&url).send().await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let body: serde_json::Value = resp.json().await?;
    Ok(draft_from_off(barcode, &body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn barcode_validation() {
        assert!(valid_barcode("3017620422003"));
        assert!(valid_barcode("12345678"));
        assert!(!valid_barcode("abc"));
        assert!(!valid_barcode("123"));
        assert!(!valid_barcode(""));
    }

    #[test]
    fn maps_off_payload() {
        let body = serde_json::json!({
            "status": 1,
            "product": {
                "product_name": "Nutella",
                "brands": "Ferrero",
                "nutriments": {
                    "energy-kcal_100g": 539.0,
                    "proteins_100g": 6.3,
                    "carbohydrates_100g": 57.5,
                    "fat_100g": 30.9,
                    "sodium_100g": 0.107,
                    "fiber_100g": "3.4",
                    "sugars_100g": 56.3
                }
            }
        });
        let d = draft_from_off("3017620422003", &body).unwrap();
        assert_eq!(d.name, "Nutella");
        assert_eq!(d.brand.as_deref(), Some("Ferrero"));
        assert_eq!(d.calories_per100g, 539.0);
        assert!((d.sodium_per100g.unwrap() - 107.0).abs() < 0.01);
    }

    #[test]
    fn status_zero_is_none() {
        let body = serde_json::json!({"status": 0});
        assert!(draft_from_off("12345678", &body).is_none());
    }
}

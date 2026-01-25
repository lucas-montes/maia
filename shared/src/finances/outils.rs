use rusqlite::{ToSql, types::FromSql};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Money(Decimal);

impl From<f64> for Money {
    fn from(amount: f64) -> Self {
        Money(Decimal::from_f64_retain(amount).unwrap_or(Decimal::ZERO))
    }
}

impl ToSql for Money {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::from(self.0.to_string()))
    }
}

impl FromSql for Money {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match value {
            rusqlite::types::ValueRef::Text(text) => {
                let s = std::str::from_utf8(text)
                    .map_err(|_| rusqlite::types::FromSqlError::InvalidType)?;
                Decimal::from_str_radix(s, 10)
                    .map_err(|_| rusqlite::types::FromSqlError::InvalidType)
                    .map(Money)
            }
            _ => Err(rusqlite::types::FromSqlError::InvalidType),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Currency {
    USD,
    EUR,
}

impl From<String> for Currency {
    fn from(s: String) -> Self {
        s.as_str().into()
    }
}

impl From<&str> for Currency {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "usd" => Currency::USD,
            "eur" => Currency::EUR,
            _ => Currency::USD, // Default to USD if unknown
        }
    }
}

impl AsRef<str> for Currency {
    fn as_ref(&self) -> &str {
        match self {
            Currency::USD => "usd",
            Currency::EUR => "eur",
        }
    }
}

impl ToSql for Currency {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::from(self.as_ref()))
    }
}

impl FromSql for Currency {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match value {
            rusqlite::types::ValueRef::Text(text) => std::str::from_utf8(text)
                .map_err(|_| rusqlite::types::FromSqlError::InvalidType)
                .map(Currency::from),
            _ => Err(rusqlite::types::FromSqlError::InvalidType),
        }
    }
}

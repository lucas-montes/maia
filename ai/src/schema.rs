//! Converting between the two structured-output dialects.
//!
//! [`crate::StructuredOutput`] and the derive macro emit Gemini's
//! `responseSchema`, which is JSON Schema plus a `propertyOrdering` keyword
//! (nullability is already written as `"type": [T, "null"]`). OpenAI-style
//! servers want plain JSON Schema and some reject unknown keywords, so the
//! extension is stripped everywhere it occurs.

use serde_json::Value;

pub fn to_json_schema(gemini: &Value) -> Value {
    match gemini {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(key, _)| key.as_str() != "propertyOrdering")
                .map(|(key, value)| (key.clone(), to_json_schema(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(to_json_schema).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn property_ordering_is_stripped_at_every_depth() {
        let gemini = json!({
            "type": "object",
            "properties": {
                "store": { "type": "string" },
                "note": { "type": ["string", "null"] },
                "products": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": { "name": { "type": "string" } },
                        "propertyOrdering": ["name"]
                    }
                }
            },
            "propertyOrdering": ["store", "note", "products"]
        });
        assert_eq!(
            to_json_schema(&gemini),
            json!({
                "type": "object",
                "properties": {
                    "store": { "type": "string" },
                    "note": { "type": ["string", "null"] },
                    "products": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": { "name": { "type": "string" } }
                        }
                    }
                }
            })
        );
    }

    /// A property literally named `propertyOrdering` is data, not the keyword…
    /// but no schema here has one, and keeping the walker simple is worth more
    /// than that corner. Pin the behaviour so a change to it is deliberate.
    #[test]
    fn a_property_named_like_the_keyword_is_dropped_too() {
        let schema = json!({ "properties": { "propertyOrdering": { "type": "string" } } });
        assert_eq!(to_json_schema(&schema), json!({ "properties": {} }));
    }
}

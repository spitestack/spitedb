//! TypeScript type generation from domain types.

use crate::ir::{DomainType, ObjectType, FieldDef};

/// Converts a DomainType to a TypeScript type string.
pub fn to_ts_type(typ: &DomainType) -> String {
    match typ {
        DomainType::String => "string".to_string(),
        DomainType::Number => "number".to_string(),
        DomainType::Boolean => "boolean".to_string(),
        DomainType::Array(inner) => format!("{}[]", to_ts_type(inner)),
        DomainType::Option(inner) => format!("{} | undefined", to_ts_type(inner)),
        DomainType::Reference(name) => name.clone(),
        DomainType::Object(obj) => generate_object_type(obj),
    }
}

/// Generates an inline TypeScript object type.
pub fn generate_object_type(obj: &ObjectType) -> String {
    if obj.fields.is_empty() {
        return "Record<string, never>".to_string();
    }

    let fields: Vec<String> = obj
        .fields
        .iter()
        .map(|f| format_field(f))
        .collect();

    format!("{{ {} }}", fields.join("; "))
}

/// Formats a single field definition.
fn format_field(field: &FieldDef) -> String {
    let optional_marker = if field.optional { "?" } else { "" };
    format!("{}{}: {}", field.name, optional_marker, to_ts_type(&field.typ))
}

/// Converts a snake_case name to PascalCase.
pub fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

/// Converts a PascalCase or camelCase name to snake_case.
pub fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}

/// Converts a name to camelCase.
#[allow(dead_code)]
pub fn to_camel_case(s: &str) -> String {
    let pascal = to_pascal_case(s);
    let mut chars = pascal.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_lowercase().chain(chars).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitive_types() {
        assert_eq!(to_ts_type(&DomainType::String), "string");
        assert_eq!(to_ts_type(&DomainType::Number), "number");
        assert_eq!(to_ts_type(&DomainType::Boolean), "boolean");
    }

    #[test]
    fn test_array_type() {
        let arr = DomainType::Array(Box::new(DomainType::String));
        assert_eq!(to_ts_type(&arr), "string[]");
    }

    #[test]
    fn test_optional_type() {
        let opt = DomainType::Option(Box::new(DomainType::Number));
        assert_eq!(to_ts_type(&opt), "number | undefined");
    }

    #[test]
    fn test_nested_array() {
        let nested = DomainType::Array(Box::new(DomainType::Array(Box::new(DomainType::String))));
        assert_eq!(to_ts_type(&nested), "string[][]");
    }

    #[test]
    fn test_object_type() {
        let obj = ObjectType {
            fields: vec![
                FieldDef {
                    name: "id".to_string(),
                    typ: DomainType::String,
                    optional: false,
                },
                FieldDef {
                    name: "count".to_string(),
                    typ: DomainType::Number,
                    optional: true,
                },
            ],
        };
        assert_eq!(
            to_ts_type(&DomainType::Object(obj)),
            "{ id: string; count?: number }"
        );
    }

    #[test]
    fn test_case_conversions() {
        assert_eq!(to_pascal_case("todo_item"), "TodoItem");
        assert_eq!(to_snake_case("TodoItem"), "todo_item");
        assert_eq!(to_camel_case("todo_item"), "todoItem");
    }
}

use serde::Deserialize;
use std::collections::HashMap;

use crate::types::units::BaseShape;

/// Raw record from Datasheets.json.
#[derive(Debug, Clone, Deserialize)]
pub struct DatasheetRecord {
    pub id: String,
    pub name: String,
}

/// Raw record from Datasheets_models.json.
#[derive(Debug, Clone, Deserialize)]
pub struct DatasheetModelRecord {
    pub datasheet_id: String,
    pub name: String,
    #[serde(rename = "M")]
    pub movement: Option<String>,
    pub base_size: Option<String>,
}

pub struct BaseDatabase {
    /// Maps unit name (lowercase) → datasheet_id.
    name_to_id: HashMap<String, String>,
    /// Maps (datasheet_id, model_name_lowercase) → model record.
    models: HashMap<(String, String), DatasheetModelRecord>,
    /// Maps datasheet_id → first model record (fallback).
    first_model: HashMap<String, DatasheetModelRecord>,
}

impl BaseDatabase {
    pub fn load(datasheets_json: &str, models_json: &str) -> Self {
        let sheets: Vec<DatasheetRecord> =
            serde_json::from_str(datasheets_json).expect("Failed to parse Datasheets.json");
        let models: Vec<DatasheetModelRecord> =
            serde_json::from_str(models_json).expect("Failed to parse Datasheets_models.json");

        let name_to_id: HashMap<String, String> = sheets
            .iter()
            .map(|s| (s.name.to_lowercase(), s.id.clone()))
            .collect();

        let mut model_map: HashMap<(String, String), DatasheetModelRecord> = HashMap::new();
        let mut first_model: HashMap<String, DatasheetModelRecord> = HashMap::new();

        for m in models {
            let key = (m.datasheet_id.clone(), m.name.to_lowercase());
            first_model
                .entry(m.datasheet_id.clone())
                .or_insert_with(|| m.clone());
            model_map.insert(key, m);
        }

        BaseDatabase {
            name_to_id,
            models: model_map,
            first_model,
        }
    }

    /// Returns true if `model_name` is a real model variant for `unit_name` in the database.
    pub fn has_model(&self, unit_name: &str, model_name: &str) -> bool {
        let Some(datasheet_id) = self.name_to_id.get(&unit_name.to_lowercase()) else {
            return false;
        };
        self.models
            .contains_key(&(datasheet_id.clone(), model_name.to_lowercase()))
    }

    /// Look up a unit by name and model variant name.
    /// Returns (BaseShape, movement_inches).
    pub fn lookup(&self, unit_name: &str, model_name: &str) -> (BaseShape, Option<f32>) {
        let datasheet_id = match self.name_to_id.get(&unit_name.to_lowercase()) {
            Some(id) => id.clone(),
            None => {
                // Try model_name as the unit name.
                match self.name_to_id.get(&model_name.to_lowercase()) {
                    Some(id) => id.clone(),
                    None => return (BaseShape::Unknown, None),
                }
            }
        };

        let record = self
            .models
            .get(&(datasheet_id.clone(), model_name.to_lowercase()))
            .or_else(|| self.first_model.get(&datasheet_id));

        match record {
            Some(r) => {
                let base = r
                    .base_size
                    .as_deref()
                    .map(parse_base_size)
                    .unwrap_or(BaseShape::Unknown);
                let movement = r.movement.as_deref().and_then(parse_movement);
                (base, movement)
            }
            None => (BaseShape::Unknown, None),
        }
    }
}

/// Parse a movement string like `"6\""` into inches.
fn parse_movement(raw: &str) -> Option<f32> {
    let trimmed = raw.trim().trim_end_matches('"').trim();
    trimmed.parse::<f32>().ok()
}

/// Parse a base_size string into a BaseShape.
/// Handles all known formats from Datasheets_models.json.
pub fn parse_base_size(raw: &str) -> BaseShape {
    // Multi-line: use only the first line.
    let line = raw.lines().next().unwrap_or(raw).trim();

    // Strip leading "Unit Name: " prefix if present (e.g., "Boss Nob: 40mm").
    let line = if let Some(pos) = line.find(':') {
        line[pos + 1..].trim()
    } else {
        line
    };

    let lower = line.to_lowercase();

    // Flying bases.
    if lower.contains("large flying base") {
        return BaseShape::FlyingBase { large: true };
    }
    if lower.contains("small flying base") || lower.contains("flying base") {
        return BaseShape::FlyingBase { large: false };
    }

    // Hull.
    if lower == "hull" || lower.starts_with("hull ") {
        return BaseShape::Hull;
    }

    // Unique / special.
    if lower.starts_with("unique") || lower == "n/a" || lower.is_empty() {
        return BaseShape::Unknown;
    }

    // Oval: "WxHmm Oval Base", "W x H mm Oval Base", etc.
    // Pattern: <number>[.][<digits>] [x|×] <number>[.][<digits>] mm
    if lower.contains("oval") || lower.contains('×') || (lower.contains('x') && lower.contains("mm")) {
        if let Some(shape) = try_parse_oval(line) {
            return shape;
        }
    }

    // Circle: "<number>mm" or "<number> mm"
    if let Some(shape) = try_parse_circle(line) {
        return shape;
    }

    BaseShape::Unknown
}

fn try_parse_oval(s: &str) -> Option<BaseShape> {
    // Normalize: replace × with x, remove spaces around x.
    let normalized = s.replace('×', "x");
    // Find two numbers separated by 'x' (case-insensitive).
    let lower = normalized.to_lowercase();
    // Strip "mm", "oval base", etc. — just extract the two leading numbers.
    let stripped = lower
        .replace("oval base", "")
        .replace("oval", "")
        .replace("mm", "")
        .trim()
        .to_string();

    let parts: Vec<&str> = stripped.split('x').collect();
    if parts.len() == 2 {
        let w = parts[0].trim().parse::<f32>().ok()?;
        let h = parts[1].trim().parse::<f32>().ok()?;
        return Some(BaseShape::Oval {
            width_mm: w,
            height_mm: h,
        });
    }
    None
}

fn try_parse_circle(s: &str) -> Option<BaseShape> {
    // Remove "mm" and any trailing text, parse the leading number.
    let cleaned = s
        .to_lowercase()
        .replace("mm", "")
        .trim()
        .split_whitespace()
        .next()?
        .trim()
        .to_string();
    let diameter = cleaned.parse::<f32>().ok()?;
    if diameter > 0.0 {
        Some(BaseShape::Circle {
            diameter_mm: diameter,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_model_rejects_wargear() {
        let base_db = BaseDatabase::load(
            include_str!("../../assets/Datasheets.json"),
            include_str!("../../assets/Datasheets_models.json"),
        );
        assert!(!base_db.has_model("Rotigus", "Gnarlrod"));
        assert!(!base_db.has_model("Skarbrand", "Bellow of endless fury"));
    }

    #[test]
    fn parse_simple_circle() {
        assert_eq!(
            parse_base_size("32mm"),
            BaseShape::Circle { diameter_mm: 32.0 }
        );
    }

    #[test]
    fn parse_decimal_circle() {
        assert_eq!(
            parse_base_size("28.5mm"),
            BaseShape::Circle { diameter_mm: 28.5 }
        );
    }

    #[test]
    fn parse_oval_nospace() {
        assert_eq!(
            parse_base_size("60x35.5mm Oval Base"),
            BaseShape::Oval {
                width_mm: 60.0,
                height_mm: 35.5
            }
        );
    }

    #[test]
    fn parse_oval_space() {
        assert_eq!(
            parse_base_size("60 x 35.5mm Oval Base"),
            BaseShape::Oval {
                width_mm: 60.0,
                height_mm: 35.5
            }
        );
    }

    #[test]
    fn parse_oval_unicode() {
        assert_eq!(
            parse_base_size("60×35mm Oval Base"),
            BaseShape::Oval {
                width_mm: 60.0,
                height_mm: 35.0
            }
        );
    }

    #[test]
    fn parse_hull() {
        assert_eq!(parse_base_size("Hull"), BaseShape::Hull);
    }

    #[test]
    fn parse_large_flying_base() {
        assert_eq!(
            parse_base_size("Large Flying Base"),
            BaseShape::FlyingBase { large: true }
        );
    }

    #[test]
    fn parse_small_flying_base() {
        assert_eq!(
            parse_base_size("Small Flying Base"),
            BaseShape::FlyingBase { large: false }
        );
    }

    #[test]
    fn parse_unique() {
        assert_eq!(parse_base_size("Unique"), BaseShape::Unknown);
    }

    #[test]
    fn parse_multiline_uses_first() {
        // Multi-line format from some entries.
        let raw = "Boss Nob: 40mm\nBreaka Boyz: 32mm";
        assert_eq!(
            parse_base_size(raw),
            BaseShape::Circle { diameter_mm: 40.0 }
        );
    }

    #[test]
    fn parse_with_colon_prefix() {
        assert_eq!(
            parse_base_size("Deathshroud: 40mm"),
            BaseShape::Circle { diameter_mm: 40.0 }
        );
    }
}

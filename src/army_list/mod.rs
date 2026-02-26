pub mod base_lookup;

use std::collections::HashMap;

/// A parsed unit from a Listforge army list.
#[derive(Debug, Clone)]
pub struct ParsedUnit {
    pub name: String,
    /// Model variant name → count.
    pub model_counts: HashMap<String, u32>,
}

/// Parse a Listforge army list text into a list of units.
///
/// Format:
/// ```text
/// Army Name - Faction Name - Detachment Name (XXXX pts)
///
/// ## Category (XXX pts)
/// Unit Name (XX pts): Option, Option
///   • Nx Model Variant: Weapon, Weapon
///   • Model Variant: Weapon
/// ```
pub fn parse_listforge(text: &str) -> Vec<ParsedUnit> {
    let mut units: Vec<ParsedUnit> = Vec::new();
    let mut current: Option<ParsedUnit> = None;

    for line in text.lines() {
        // Skip empty lines, header (first line with faction), and category headers.
        if line.trim().is_empty() {
            continue;
        }
        if line.trim_start().starts_with("##") {
            continue;
        }
        // Skip army header line: "Army - Faction - Detachment (pts)"
        // Detected by containing " - " (space-dash-space).
        if line.contains(" - ") {
            continue;
        }

        // Model variant line: 2+ spaces then bullet (•).
        if let Some(model_line) = strip_bullet(line) {
            if let Some(ref mut unit) = current {
                let (count, model_name) = parse_count_and_name(model_line);
                *unit.model_counts.entry(model_name).or_insert(0) += count;
            }
            continue;
        }

        // Parent unit line: matches "Unit Name (NNN pts)" optionally followed by ": ..."
        if let Some(unit_name) = parse_unit_line(line) {
            if let Some(prev) = current.take() {
                units.push(prev);
            }
            current = Some(ParsedUnit {
                name: unit_name,
                model_counts: HashMap::new(),
            });
            continue;
        }
    }

    if let Some(last) = current {
        units.push(last);
    }

    // For units with no model variants, insert the unit name itself as a single model.
    for unit in &mut units {
        if unit.model_counts.is_empty() {
            unit.model_counts.insert(unit.name.clone(), 1);
        }
    }

    units
}

/// Strip leading whitespace + bullet (•) from a model variant line.
/// Returns the content after the bullet, or None if not a bullet line.
fn strip_bullet(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    // Require that original line had at least 2 spaces of indent.
    let indent = line.len() - trimmed.len();
    if indent < 2 {
        return None;
    }
    trimmed.strip_prefix('•').map(|s| s.trim())
}

/// Parse "Unit Name (NNN pts)[: ...]" → Some("Unit Name"), or None.
fn parse_unit_line(line: &str) -> Option<String> {
    let line = line.trim();
    // Pattern: text ending with "(NNN pts)" optionally followed by ": ..."
    let close_paren = line.rfind(')')?;
    let open_paren = line[..close_paren].rfind('(')?;
    let inside = line[open_paren + 1..close_paren].trim();
    // Validate: "NNN pts" where NNN is digits.
    let pts_part = inside.strip_suffix("pts")?.trim();
    pts_part.parse::<u32>().ok()?;
    let name = line[..open_paren].trim().trim_end_matches(':').trim();
    if name.is_empty() {
        return None;
    }
    // Reject category lines which start with ##.
    if name.starts_with('#') {
        return None;
    }
    Some(name.to_string())
}

/// Parse "Nx Model Name: weapons..." or "Model Name: weapons..." → (count, name).
fn parse_count_and_name(s: &str) -> (u32, String) {
    // Strip weapon list (everything after first colon).
    let name_part = s.split(':').next().unwrap_or(s).trim();

    // Look for leading count like "4x " or "4X ".
    if let Some(idx) = name_part.find('x').or_else(|| name_part.find('X')) {
        let count_str = &name_part[..idx];
        if let Ok(n) = count_str.trim().parse::<u32>() {
            let name = name_part[idx + 1..].trim().to_string();
            if !name.is_empty() {
                return (n, name);
            }
        }
    }

    (1, name_part.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_LIST: &str = r#"My Army - Space Marines - Gladius Task Force (1000 pts)

## Battleline (500 pts)
Intercessor Squad (100 pts): Sergeant with auspex
  • 4x Intercessor: Bolt rifle
  • Intercessor Sergeant: Bolt rifle, Auspex

Tactical Squad (100 pts)
  • 9x Tactical Marine: Boltgun

## HQ (250 pts)
Captain (90 pts)
"#;

    #[test]
    fn parses_units() {
        let units = parse_listforge(SAMPLE_LIST);
        assert_eq!(units.len(), 3, "Expected 3 units");
    }

    #[test]
    fn parses_model_counts() {
        let units = parse_listforge(SAMPLE_LIST);
        let intercessors = units.iter().find(|u| u.name == "Intercessor Squad").unwrap();
        assert_eq!(intercessors.model_counts["Intercessor"], 4);
        assert_eq!(intercessors.model_counts["Intercessor Sergeant"], 1);
    }

    #[test]
    fn unit_with_no_models_gets_self_count() {
        let units = parse_listforge(SAMPLE_LIST);
        let captain = units.iter().find(|u| u.name == "Captain").unwrap();
        assert_eq!(captain.model_counts["Captain"], 1);
    }

    #[test]
    fn parses_nx_prefix() {
        let units = parse_listforge(SAMPLE_LIST);
        let tactical = units.iter().find(|u| u.name == "Tactical Squad").unwrap();
        assert_eq!(tactical.model_counts["Tactical Marine"], 9);
    }
}

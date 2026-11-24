//! Generates `crates/vnce-raster/src/mode/tables.rs` from the official Unicode
//! character database, so that no block character is typed in by hand.
//!
//! Usage:
//!
//! ```sh
//! curl -O https://www.unicode.org/Public/UCD/latest/ucd/UnicodeData.txt
//! cargo run --manifest-path tools/gen-block-tables/Cargo.toml -- UnicodeData.txt \
//!     > crates/vnce-raster/src/mode/tables.rs
//! ```
//!
//! Cells are numbered row by row, left to right, starting at 1 (the numbering used in the
//! Unicode character names): bit `n - 1` of a pattern is cell `n`.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::process::ExitCode;

/// Quadrant patterns (cells 1 2 / 3 4) and the characters that draw them.
const QUADRANTS: [(&str, &str); 16] = [
    ("", "SPACE"),
    ("1", "QUADRANT UPPER LEFT"),
    ("2", "QUADRANT UPPER RIGHT"),
    ("12", "UPPER HALF BLOCK"),
    ("3", "QUADRANT LOWER LEFT"),
    ("13", "LEFT HALF BLOCK"),
    ("23", "QUADRANT UPPER RIGHT AND LOWER LEFT"),
    ("123", "QUADRANT UPPER LEFT AND UPPER RIGHT AND LOWER LEFT"),
    ("4", "QUADRANT LOWER RIGHT"),
    ("14", "QUADRANT UPPER LEFT AND LOWER RIGHT"),
    ("24", "RIGHT HALF BLOCK"),
    ("124", "QUADRANT UPPER LEFT AND UPPER RIGHT AND LOWER RIGHT"),
    ("34", "LOWER HALF BLOCK"),
    ("134", "QUADRANT UPPER LEFT AND LOWER LEFT AND LOWER RIGHT"),
    ("234", "QUADRANT UPPER RIGHT AND LOWER LEFT AND LOWER RIGHT"),
    ("1234", "FULL BLOCK"),
];

/// Sextant patterns (cells 1 2 / 3 4 / 5 6) that are not named `BLOCK SEXTANT-…`.
const SEXTANT_EXTRA: [(&str, &str); 4] = [
    ("", "SPACE"),
    ("135", "LEFT HALF BLOCK"),
    ("246", "RIGHT HALF BLOCK"),
    ("123456", "FULL BLOCK"),
];

/// Octant patterns (cells 1 2 / 3 4 / 5 6 / 7 8) that are not named `BLOCK OCTANT-…`.
const OCTANT_EXTRA: [(&str, &str); 26] = [
    ("", "SPACE"),
    ("1", "LEFT HALF UPPER ONE QUARTER BLOCK"),
    ("2", "RIGHT HALF UPPER ONE QUARTER BLOCK"),
    ("12", "UPPER ONE QUARTER BLOCK"),
    ("13", "QUADRANT UPPER LEFT"),
    ("24", "QUADRANT UPPER RIGHT"),
    ("1234", "UPPER HALF BLOCK"),
    ("35", "MIDDLE LEFT ONE QUARTER BLOCK"),
    ("46", "MIDDLE RIGHT ONE QUARTER BLOCK"),
    ("123456", "UPPER THREE QUARTERS BLOCK"),
    ("7", "LEFT HALF LOWER ONE QUARTER BLOCK"),
    ("57", "QUADRANT LOWER LEFT"),
    ("1357", "LEFT HALF BLOCK"),
    ("2457", "QUADRANT UPPER RIGHT AND LOWER LEFT"),
    (
        "123457",
        "QUADRANT UPPER LEFT AND UPPER RIGHT AND LOWER LEFT",
    ),
    ("8", "RIGHT HALF LOWER ONE QUARTER BLOCK"),
    ("68", "QUADRANT LOWER RIGHT"),
    ("1368", "QUADRANT UPPER LEFT AND LOWER RIGHT"),
    ("2468", "RIGHT HALF BLOCK"),
    (
        "123468",
        "QUADRANT UPPER LEFT AND UPPER RIGHT AND LOWER RIGHT",
    ),
    ("78", "LOWER ONE QUARTER BLOCK"),
    ("5678", "LOWER HALF BLOCK"),
    (
        "135678",
        "QUADRANT UPPER LEFT AND LOWER LEFT AND LOWER RIGHT",
    ),
    (
        "245678",
        "QUADRANT UPPER RIGHT AND LOWER LEFT AND LOWER RIGHT",
    ),
    ("345678", "LOWER THREE QUARTERS BLOCK"),
    ("12345678", "FULL BLOCK"),
];

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: gen-block-tables <UnicodeData.txt>");
        return ExitCode::FAILURE;
    };
    let data = match std::fs::read_to_string(&path) {
        Ok(data) => data,
        Err(error) => {
            eprintln!("cannot read {path}: {error}");
            return ExitCode::FAILURE;
        }
    };
    match generate(&data) {
        Ok(source) => {
            print!("{source}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Character names to characters.
fn parse(data: &str) -> Result<HashMap<String, char>, String> {
    let mut names = HashMap::new();
    for line in data.lines() {
        let mut fields = line.split(';');
        let (Some(code), Some(name)) = (fields.next(), fields.next()) else {
            continue;
        };
        let code =
            u32::from_str_radix(code, 16).map_err(|e| format!("bad code point {code}: {e}"))?;
        if let Some(ch) = char::from_u32(code) {
            names.insert(name.to_owned(), ch);
        }
    }
    Ok(names)
}

/// Converts cell digits ("135") to a bit pattern.
fn pattern(digits: &str, cells: u32) -> Result<usize, String> {
    let mut mask = 0;
    for digit in digits.chars() {
        let cell = digit
            .to_digit(10)
            .filter(|&d| d >= 1 && d <= cells)
            .ok_or_else(|| format!("bad cell `{digit}` in `{digits}`"))?;
        mask |= 1 << (cell - 1);
    }
    Ok(mask)
}

/// Builds a table of `2^cells` characters from `BLOCK <prefix>-<digits>` names plus extra
/// named characters, checking that every pattern gets exactly one unique character.
fn table(
    names: &HashMap<String, char>,
    prefix: Option<&str>,
    extra: &[(&str, &str)],
    cells: u32,
) -> Result<Vec<char>, String> {
    let size = 1 << cells;
    let mut table: Vec<Option<char>> = vec![None; size];
    let mut assign = |mask: usize, ch: char, source: &str| -> Result<(), String> {
        if let Some(previous) = table[mask] {
            return Err(format!(
                "pattern {mask:#b} mapped twice ({previous:?} and {source})"
            ));
        }
        table[mask] = Some(ch);
        Ok(())
    };
    if let Some(prefix) = prefix {
        let full = format!("BLOCK {prefix}-");
        let mut sorted: Vec<_> = names.iter().filter(|(n, _)| n.starts_with(&full)).collect();
        sorted.sort();
        for (name, &ch) in sorted {
            let mask = pattern(&name[full.len()..], cells)?;
            assign(mask, ch, name)?;
        }
    }
    for &(digits, name) in extra {
        let ch = if name == "SPACE" {
            ' '
        } else {
            *names
                .get(name)
                .ok_or_else(|| format!("missing character {name}"))?
        };
        assign(pattern(digits, cells)?, ch, name)?;
    }
    let table: Vec<char> = table
        .into_iter()
        .enumerate()
        .map(|(mask, ch)| ch.ok_or_else(|| format!("pattern {mask:#b} has no character")))
        .collect::<Result<_, _>>()?;
    let unique: HashSet<char> = table.iter().copied().collect();
    if unique.len() != table.len() {
        return Err("characters are not unique".into());
    }
    Ok(table)
}

fn emit(out: &mut String, name: &str, doc: &str, table: &[char]) {
    let _ = writeln!(out, "/// {doc}");
    // Keep the generated layout, so the file always matches the generator output.
    let _ = writeln!(out, "#[rustfmt::skip]");
    let _ = writeln!(out, "pub(crate) const {name}: [char; {}] = [", table.len());
    for chunk in table.chunks(8) {
        let line: Vec<String> = chunk
            .iter()
            .map(|&ch| format!("'\\u{{{:x}}}'", u32::from(ch)))
            .collect();
        let _ = writeln!(out, "    {},", line.join(", "));
    }
    let _ = writeln!(out, "];");
}

fn generate(data: &str) -> Result<String, String> {
    let names = parse(data)?;
    let quadrant = table(&names, None, &QUADRANTS, 4)?;
    let sextant = table(&names, Some("SEXTANT"), &SEXTANT_EXTRA, 6)?;
    let octant = table(&names, Some("OCTANT"), &OCTANT_EXTRA, 8)?;

    let mut out = String::new();
    out.push_str(
        "// @generated by tools/gen-block-tables from the Unicode character database\n\
         // (UnicodeData.txt). Do not edit by hand; run the generator instead.\n\
         //\n\
         // Index = bit pattern of filled cells; bit `n - 1` is cell `n`, numbered row by row\n\
         // and left to right as in the Unicode character names.\n\n",
    );
    emit(
        &mut out,
        "QUADRANT",
        "Quadrant characters (2×2 cells).",
        &quadrant,
    );
    out.push('\n');
    emit(
        &mut out,
        "SEXTANT",
        "Sextant characters (2×3 cells, Unicode 13).",
        &sextant,
    );
    out.push('\n');
    emit(
        &mut out,
        "OCTANT",
        "Octant characters (2×4 cells, Unicode 16).",
        &octant,
    );
    Ok(out)
}

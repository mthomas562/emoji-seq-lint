//! Parsing and structural validation for `.seq` files: plain-text
//! definitions of named emoji sequences, one per line, like
//!
//!     :flag_us: 1F1FA 1F1F8
//!     :family_mwgb: 1F468 200D 1F469 200D 1F466 200D 1F466
//!
//! The parser tracks byte-accurate line and column positions so that
//! every error can point at the exact token that caused it.

use std::collections::HashMap;

/// A location and width within a source line, used to draw carets
/// under the offending token in a rendered `Diagnostic`.
#[derive(Debug, Clone, Copy)]
pub struct CodepointSpan {
    pub column: usize,
    pub length: usize,
}

/// One parsed line: a name plus the codepoints that make up its sequence.
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub name_span: CodepointSpan,
    pub codepoints: Vec<char>,
    pub positions: Vec<CodepointSpan>,
    pub line: usize,
}

/// A parse or validation error with enough position information to
/// render a compiler-style message.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub line: usize,
    pub column: usize,
    pub length: usize,
    pub message: String,
}

impl Diagnostic {
    /// Render this diagnostic the way a compiler would: the message,
    /// the file:line:column, the offending source line, and carets
    /// under the exact span that is wrong.
    pub fn render(&self, filename: &str, source: &str) -> String {
        let source_line = source.lines().nth(self.line.saturating_sub(1)).unwrap_or("");
        let gutter_width = self.line.to_string().len();
        let gutter = " ".repeat(gutter_width);

        // Preserve tabs in the padding so carets line up under wide
        // characters and mixed indentation the same way the source does.
        let pad: String = source_line
            .chars()
            .take(self.column.saturating_sub(1))
            .map(|c| if c == '\t' { '\t' } else { ' ' })
            .collect();
        let carets = "^".repeat(self.length.max(1));

        let mut out = String::new();
        out.push_str(&format!("error: {}\n", self.message));
        out.push_str(&format!("{} --> {}:{}:{}\n", gutter, filename, self.line, self.column));
        out.push_str(&format!("{} |\n", gutter));
        out.push_str(&format!("{} | {}\n", self.line, source_line));
        out.push_str(&format!("{} | {}{}\n", gutter, pad, carets));
        out
    }
}

/// The shape a validated sequence falls into, per the Unicode emoji
/// sequence categories (single, ZWJ, flag, modifier, keycap, tag).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceKind {
    Single,
    ZwjSequence,
    Flag,
    Modifier,
    Keycap,
    Tag,
}

const ZWJ: char = '\u{200D}';
const VS16: char = '\u{FE0F}';
const KEYCAP_COMBINER: char = '\u{20E3}';
const TAG_BASE: char = '\u{1F3F4}';
const CANCEL_TAG: char = '\u{E007F}';

fn is_regional_indicator(c: char) -> bool {
    ('\u{1F1E6}'..='\u{1F1FF}').contains(&c)
}

fn is_skin_tone_modifier(c: char) -> bool {
    ('\u{1F3FB}'..='\u{1F3FF}').contains(&c)
}

fn is_keycap_base(c: char) -> bool {
    c.is_ascii_digit() || c == '#' || c == '*'
}

fn is_tag_char(c: char) -> bool {
    ('\u{E0020}'..='\u{E007E}').contains(&c)
}

/// Parse every non-blank, non-comment line in `source` into an `Entry`.
/// Lines that fail to parse produce a `Diagnostic` instead of aborting
/// the whole file, so a single run reports every problem at once.
pub fn parse(source: &str) -> (Vec<Entry>, Vec<Diagnostic>) {
    let mut entries = Vec::new();
    let mut diagnostics = Vec::new();

    for (idx, raw_line) in source.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        match parse_line(raw_line, line_no) {
            Ok(entry) => entries.push(entry),
            Err(diag) => diagnostics.push(diag),
        }
    }

    (entries, diagnostics)
}

fn parse_line(raw_line: &str, line_no: usize) -> Result<Entry, Diagnostic> {
    let chars: Vec<char> = raw_line.chars().collect();
    let mut i = 0;

    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }

    if i >= chars.len() || chars[i] != ':' {
        return Err(Diagnostic {
            line: line_no,
            column: i + 1,
            length: 1,
            message: "expected an entry name starting with ':'".to_string(),
        });
    }

    let name_start = i;
    i += 1;
    while i < chars.len() && chars[i] != ':' {
        i += 1;
    }
    if i >= chars.len() {
        return Err(Diagnostic {
            line: line_no,
            column: name_start + 1,
            length: chars.len() - name_start,
            message: "unterminated name, expected a closing ':'".to_string(),
        });
    }
    i += 1;
    let name: String = chars[name_start..i].iter().collect();
    let name_span = CodepointSpan {
        column: name_start + 1,
        length: i - name_start,
    };

    for (offset, c) in chars[name_start + 1..i - 1].iter().enumerate() {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_' || *c == '-' || *c == '+') {
            return Err(Diagnostic {
                line: line_no,
                column: name_start + 2 + offset,
                length: 1,
                message: format!(
                    "invalid character '{}' in name; use lowercase letters, digits, '_', '-', or '+'",
                    c
                ),
            });
        }
    }

    let after_name = i;
    if i >= chars.len() {
        return Err(Diagnostic {
            line: line_no,
            column: after_name + 1,
            length: 1,
            message: "expected at least one codepoint after the name".to_string(),
        });
    }
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    if i == after_name {
        return Err(Diagnostic {
            line: line_no,
            column: after_name + 1,
            length: 1,
            message: "expected whitespace between the name and its codepoints".to_string(),
        });
    }

    let mut codepoints = Vec::new();
    let mut positions = Vec::new();
    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let tok_start = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        let token: String = chars[tok_start..i].iter().collect();
        let hex = token
            .strip_prefix("U+")
            .or_else(|| token.strip_prefix("u+"))
            .unwrap_or(&token);

        let value = u32::from_str_radix(hex, 16).map_err(|_| Diagnostic {
            line: line_no,
            column: tok_start + 1,
            length: i - tok_start,
            message: format!("\"{}\" is not a valid hexadecimal codepoint", token),
        })?;

        let ch = char::from_u32(value).ok_or_else(|| Diagnostic {
            line: line_no,
            column: tok_start + 1,
            length: i - tok_start,
            message: format!("U+{:04X} is not a valid Unicode scalar value", value),
        })?;

        codepoints.push(ch);
        positions.push(CodepointSpan {
            column: tok_start + 1,
            length: i - tok_start,
        });
    }

    if codepoints.is_empty() {
        return Err(Diagnostic {
            line: line_no,
            column: i + 1,
            length: 1,
            message: "expected at least one codepoint after the name".to_string(),
        });
    }

    Ok(Entry {
        name,
        name_span,
        codepoints,
        positions,
        line: line_no,
    })
}

/// Find entries whose `:name:` repeats within the same file. The first
/// definition of a name is treated as authoritative; every later
/// occurrence is reported, pointing at the repeated name and noting
/// where it was first defined.
pub fn find_duplicates(entries: &[Entry]) -> Vec<Diagnostic> {
    let mut first_seen: HashMap<&str, usize> = HashMap::new();
    let mut diagnostics = Vec::new();

    for entry in entries {
        match first_seen.get(entry.name.as_str()) {
            Some(&first_line) => diagnostics.push(Diagnostic {
                line: entry.line,
                column: entry.name_span.column,
                length: entry.name_span.length,
                message: format!(
                    "duplicate entry name \"{}\"; first defined on line {}",
                    entry.name, first_line
                ),
            }),
            None => {
                first_seen.insert(entry.name.as_str(), entry.line);
            }
        }
    }

    diagnostics
}

/// Check that an entry's codepoints form one of the recognized emoji
/// sequence shapes. This is a structural check only: it confirms the
/// grammar (ZWJ joins, flag pairs, modifier pairs, keycap and tag
/// endings) but does not cross-reference the Unicode registry of
/// recommended-for-general-interchange sequences.
pub fn classify(entry: &Entry) -> Result<SequenceKind, Diagnostic> {
    let cps = &entry.codepoints;

    if cps.len() == 2 && cps.iter().all(|c| is_regional_indicator(*c)) {
        return Ok(SequenceKind::Flag);
    }

    if is_keycap_base(cps[0]) {
        let rest = &cps[1..];
        let with_vs16: [char; 2] = [VS16, KEYCAP_COMBINER];
        let without_vs16: [char; 1] = [KEYCAP_COMBINER];
        if rest == with_vs16 || rest == without_vs16 {
            return Ok(SequenceKind::Keycap);
        }
    }

    if cps[0] == TAG_BASE && cps.len() >= 3 {
        let last = *cps.last().unwrap();
        let middle = &cps[1..cps.len() - 1];
        if last == CANCEL_TAG && middle.iter().all(|c| is_tag_char(*c)) {
            return Ok(SequenceKind::Tag);
        }
    }

    if cps.len() == 2 && is_skin_tone_modifier(cps[1]) && !is_skin_tone_modifier(cps[0]) {
        return Ok(SequenceKind::Modifier);
    }

    if cps.contains(&ZWJ) {
        return classify_zwj(entry);
    }

    if cps.len() == 1 {
        return Ok(SequenceKind::Single);
    }

    let span = &entry.positions[1];
    Err(Diagnostic {
        line: entry.line,
        column: span.column,
        length: span.length,
        message: format!(
            "\"{}\" does not match a known sequence shape (single, ZWJ, flag, modifier, keycap, or tag)",
            entry.name
        ),
    })
}

fn classify_zwj(entry: &Entry) -> Result<SequenceKind, Diagnostic> {
    let cps = &entry.codepoints;

    if cps[0] == ZWJ {
        let span = &entry.positions[0];
        return Err(Diagnostic {
            line: entry.line,
            column: span.column,
            length: span.length,
            message: "a ZWJ sequence cannot start with a zero-width joiner".to_string(),
        });
    }

    if *cps.last().unwrap() == ZWJ {
        let span = entry.positions.last().unwrap();
        return Err(Diagnostic {
            line: entry.line,
            column: span.column,
            length: span.length,
            message: "a ZWJ sequence cannot end with a zero-width joiner".to_string(),
        });
    }

    for i in 1..cps.len() {
        if cps[i] == ZWJ && cps[i - 1] == ZWJ {
            let span = &entry.positions[i];
            return Err(Diagnostic {
                line: entry.line,
                column: span.column,
                length: span.length,
                message: "two consecutive zero-width joiners".to_string(),
            });
        }
    }

    Ok(SequenceKind::ZwjSequence)
}

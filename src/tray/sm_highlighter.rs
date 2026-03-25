use crate::sprite::sm_compiler::{ActionType, Direction};
use crate::sprite::sm_format::{
    STATE_FIELD_NAMES, TRANSITION_FIELD_NAMES, META_FIELD_NAMES, INTERRUPT_FIELD_NAMES,
};
use eframe::egui::{Color32, FontId};
use eframe::egui::text::{LayoutJob, TextFormat};

pub struct PetstateTheme {
    pub font_id:       FontId,
    pub comment:       Color32,  // # comments
    pub section:       Color32,  // [meta], [interrupts], [states.x] brackets + words
    pub state_name:    Color32,  // the NAME part in [states.NAME]
    pub field_known:   Color32,  // recognized field names
    pub field_unknown: Color32,  // unrecognized field names (still readable)
    pub action_val:    Color32,  // "idle", "walk", etc. after action= or dir=
    pub state_ref:     Color32,  // goto/fallback string values (state names)
    pub special:       Color32,  // "$previous" literal
    pub duration_val:  Color32,  // "500ms", "1s-3s"
    pub bool_val:      Color32,  // true / false
    pub number:        Color32,  // 123, 45.6
    pub string:        Color32,  // other quoted strings
    pub operator:      Color32,  // = , [ ] { }
    pub default:       Color32,  // plain whitespace/unmatched
}

impl PetstateTheme {
    pub fn dark(font_id: FontId) -> Self {
        Self {
            font_id,
            comment:       Color32::from_rgb(106, 153,  85),  // muted green
            section:       Color32::from_rgb(197, 134, 192),  // purple
            state_name:    Color32::from_rgb(220, 220, 170),  // yellow
            field_known:   Color32::from_rgb(156, 220, 254),  // light blue
            field_unknown: Color32::from_rgb(212, 212, 212),  // light gray
            action_val:    Color32::from_rgb( 78, 201, 176),  // teal
            state_ref:     Color32::from_rgb(220, 220, 170),  // yellow (same as state_name)
            special:       Color32::from_rgb(255, 185,  50),  // bright gold (distinct from string)
            duration_val:  Color32::from_rgb(197, 134, 192),  // purple
            bool_val:      Color32::from_rgb( 86, 156, 214),  // blue
            number:        Color32::from_rgb(181, 206, 168),  // light green
            string:        Color32::from_rgb(206, 145, 120),  // orange/salmon
            operator:      Color32::from_rgb(212, 212, 212),  // gray
            default:       Color32::from_rgb(212, 212, 212),  // gray
        }
    }

    pub fn light(font_id: FontId) -> Self {
        Self {
            font_id,
            comment:       Color32::from_rgb( 57,  98,  18),  // dark green
            section:       Color32::from_rgb(113,  56, 127),  // dark purple
            state_name:    Color32::from_rgb(130,  85,   0),  // dark yellow/brown
            field_known:   Color32::from_rgb(  0, 112, 193),  // dark blue
            field_unknown: Color32::from_rgb( 80,  80,  80),  // dark gray
            action_val:    Color32::from_rgb(  0, 128, 100),  // dark teal
            state_ref:     Color32::from_rgb(130,  85,   0),  // dark yellow
            special:       Color32::from_rgb(190, 140,   0),  // dark gold (distinct from string)
            duration_val:  Color32::from_rgb(113,  56, 127),  // dark purple
            bool_val:      Color32::from_rgb(  0,   0, 200),  // dark blue
            number:        Color32::from_rgb( 50, 130,  50),  // dark green
            string:        Color32::from_rgb(163,  52,  20),  // dark orange
            operator:      Color32::from_rgb( 80,  80,  80),  // dark gray
            default:       Color32::from_rgb( 30,  30,  30),  // near black
        }
    }
}

pub fn highlight_petstate(code: &str, theme: &PetstateTheme) -> LayoutJob {
    let mut job = LayoutJob::default();
    let mut in_transitions: bool = false;

    for line in code.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let indent_len = line.len() - trimmed.len();
        let indent = &line[..indent_len];

        if trimmed.is_empty() || trimmed == "\n" {
            push(&mut job, line, theme.default, &theme.font_id);
            continue;
        }

        if trimmed.starts_with('#') {
            push(&mut job, indent, theme.default, &theme.font_id);
            push(&mut job, trimmed, theme.comment, &theme.font_id);
            continue;
        }

        // Section header: [meta], [states.idle], etc.
        if trimmed.starts_with('[') && !in_transitions {
            in_transitions = false;
            colorize_section_line(&mut job, line, theme);
            continue;
        }

        // Closing ] for transitions array — strip trailing comment/comma before comparing
        let effective = trimmed.split('#').next().unwrap_or("").trim().trim_end_matches(',').trim();
        if effective == "]" && in_transitions {
            push(&mut job, indent, theme.default, &theme.font_id);
            push(&mut job, "]", theme.operator, &theme.font_id);
            // emit anything after ] (comma, whitespace, comment)
            let rest_of_line = &line[indent_len + 1..];  // skip indent + "]"
            if let Some(hash) = rest_of_line.find('#') {
                push(&mut job, &rest_of_line[..hash], theme.default, &theme.font_id);
                push(&mut job, &rest_of_line[hash..], theme.comment, &theme.font_id);
            } else {
                push(&mut job, rest_of_line, theme.default, &theme.font_id);
            }
            in_transitions = false;
            continue;
        }

        // Inline table entry inside transitions: { goto = "x", after = "1s" },
        if in_transitions && (trimmed.starts_with('{') || trimmed.starts_with(']')) {
            push(&mut job, indent, theme.default, &theme.font_id);
            colorize_inline_table(&mut job, trimmed, theme);
            continue;
        }

        // Key = value line
        if let Some(eq_pos) = find_top_level_eq(trimmed) {
            let key = trimmed[..eq_pos].trim();
            let rest = trimmed[eq_pos + 1..].trim();
            push(&mut job, indent, theme.default, &theme.font_id);
            // Color the key
            let all_fields: &[&[&str]] = &[
                STATE_FIELD_NAMES, META_FIELD_NAMES,
                TRANSITION_FIELD_NAMES, INTERRUPT_FIELD_NAMES,
            ];
            let known = all_fields.iter().any(|arr| arr.contains(&key));
            push(&mut job, key, if known { theme.field_known } else { theme.field_unknown }, &theme.font_id);
            // eq_region: everything from end of key up to and including '='
            let eq_region = &trimmed[key.len()..eq_pos + 1];
            push(&mut job, eq_region, theme.operator, &theme.font_id);
            // Color the value
            if let Some(after_open) = rest.strip_prefix('[') {
                in_transitions = true;
                push(&mut job, "[", theme.operator, &theme.font_id);
                let after_bracket = after_open.trim();
                if !after_bracket.is_empty() && after_bracket != "\n" {
                    colorize_inline_table(&mut job, after_bracket, theme);
                } else {
                    // trailing newline after '['
                    if line.ends_with('\n') {
                        push(&mut job, "\n", theme.default, &theme.font_id);
                    }
                }
            } else {
                colorize_value(&mut job, rest, key, theme);
                // colorize_value doesn't emit the trailing newline if rest was trimmed
                // We need to emit the newline that split_inclusive gives us
                if line.ends_with('\n') && !rest.ends_with('\n') {
                    push(&mut job, "\n", theme.default, &theme.font_id);
                }
            }
            continue;
        }

        // Fallback
        push(&mut job, line, theme.default, &theme.font_id);
    }

    job
}

fn push(job: &mut LayoutJob, text: &str, color: Color32, font_id: &FontId) {
    if text.is_empty() { return; }
    job.append(text, 0.0, TextFormat { font_id: font_id.clone(), color, ..Default::default() });
}

/// Find the position of `=` that is NOT inside quotes or brackets (top-level key=value)
fn find_top_level_eq(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut str_char = '"';
    for (i, c) in s.char_indices() {
        if in_str {
            if c == str_char { in_str = false; }
            continue;
        }
        match c {
            '"' | '\'' => { in_str = true; str_char = c; }
            '[' | '{' => depth += 1,
            ']' | '}' => depth -= 1,
            '=' if depth == 0 => return Some(i),
            '#' => return None, // rest is comment
            _ => {}
        }
    }
    None
}

/// Color a value string given the context key
fn colorize_value(job: &mut LayoutJob, value: &str, key: &str, theme: &PetstateTheme) {
    // Handle trailing newline
    let (value_body, newline) = if let Some(stripped) = value.strip_suffix('\n') {
        (stripped, "\n")
    } else {
        (value, "")
    };

    if value_body == "true" || value_body == "false" {
        push(job, value_body, theme.bool_val, &theme.font_id);
    } else if is_number(value_body) {
        push(job, value_body, theme.number, &theme.font_id);
    } else if value_body.starts_with('"') || value_body.starts_with('\'') {
        // Extract inner content
        let inner = strip_quotes(value_body);
        let quote_char = &value_body[..1];
        push(job, quote_char, theme.operator, &theme.font_id);
        let color = value_color(inner, key, theme);
        push(job, inner, color, &theme.font_id);
        push(job, quote_char, theme.operator, &theme.font_id);
    } else {
        push(job, value_body, theme.default, &theme.font_id);
    }
    push(job, newline, theme.default, &theme.font_id);
}

/// Determine color for a quoted string's inner content based on context key
fn value_color(inner: &str, key: &str, theme: &PetstateTheme) -> Color32 {
    match key {
        "action" => {
            if ActionType::ALL.iter().any(|a| a.as_str() == inner) {
                theme.action_val
            } else {
                theme.string
            }
        }
        "dir" => {
            if Direction::ALL.iter().any(|d| d.as_str() == inner) {
                theme.action_val
            } else {
                theme.string
            }
        }
        "goto" | "fallback" | "default_fallback" => {
            if inner == "$previous" {
                theme.special
            } else {
                theme.state_ref
            }
        }
        "duration" | "after" => {
            if is_duration(inner) { theme.duration_val } else { theme.string }
        }
        _ => theme.string,
    }
}

fn strip_quotes(s: &str) -> &str {
    if (s.starts_with('"') && s.ends_with('"')) ||
       (s.starts_with('\'') && s.ends_with('\'')) {
        &s[1..s.len()-1]
    } else {
        s
    }
}

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-' || c == 'e' || c == 'E' || c == '+')
    && s.chars().any(|c| c.is_ascii_digit())
}

fn is_duration(s: &str) -> bool {
    // "500ms", "3s", "2.5s", "1s-3s", "500ms-2000ms"
    if let Some(mid) = s.find('-').filter(|&i| i > 0) {
        let (a, b) = (&s[..mid], &s[mid+1..]);
        return is_duration_single(a) && is_duration_single(b);
    }
    is_duration_single(s)
}

fn is_duration_single(s: &str) -> bool {
    let s = s.trim();
    let num_part = if let Some(stripped) = s.strip_suffix("ms") {
        stripped
    } else if let Some(stripped) = s.strip_suffix('s') {
        stripped
    } else {
        return false;
    };
    !num_part.is_empty() && num_part.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// Colorize a section header line like "[states.idle]" or "[meta]"
fn colorize_section_line(job: &mut LayoutJob, line: &str, theme: &PetstateTheme) {
    let trimmed = line.trim_start();
    let indent_len = line.len() - trimmed.len();
    push(job, &line[..indent_len], theme.default, &theme.font_id);

    // Find [ and ]
    let content = trimmed.trim_end_matches(['\n', '\r', ' ']);
    // content like "[states.idle]" or "[meta]"
    if let (Some(open), Some(close)) = (content.find('['), content.rfind(']')) {
        push(job, &content[..=open], theme.section, &theme.font_id);  // "["
        let inner = &content[open+1..close];  // "states.idle" or "meta"
        // Check if it's "states.NAME" or "interrupts.NAME"
        if let Some(dot) = inner.find('.') {
            let prefix = &inner[..=dot];  // "states."
            let name = &inner[dot+1..];   // "idle"
            push(job, prefix, theme.section, &theme.font_id);
            push(job, name, theme.state_name, &theme.font_id);
        } else {
            push(job, inner, theme.section, &theme.font_id);
        }
        push(job, "]", theme.section, &theme.font_id);
        // trailing newline
        if content.len() < trimmed.len() {
            push(job, &trimmed[content.len()..], theme.default, &theme.font_id);
        } else if line.ends_with('\n') {
            push(job, "\n", theme.default, &theme.font_id);
        }
    } else {
        push(job, trimmed, theme.default, &theme.font_id);
    }
}

/// Colorize an inline table { goto = "x", after = "1s" } or part of transitions array
fn colorize_inline_table(job: &mut LayoutJob, s: &str, theme: &PetstateTheme) {
    let mut i = 0;
    while i < s.len() {
        let c = s[i..].chars().next().unwrap();
        match c {
            '{' | '}' | '[' | ']' | ',' => {
                push(job, &s[i..i+1], theme.operator, &theme.font_id);
                i += 1;
            }
            ' ' | '\t' | '\n' | '\r' => {
                let start = i;
                while i < s.len() {
                    let ch = s[i..].chars().next().unwrap();
                    if matches!(ch, ' ' | '\t' | '\n' | '\r') {
                        i += ch.len_utf8();
                    } else {
                        break;
                    }
                }
                push(job, &s[start..i], theme.default, &theme.font_id);
            }
            '#' => {
                push(job, &s[i..], theme.comment, &theme.font_id);
                break;
            }
            _ => {
                // Try to parse key = value
                if let Some(eq_offset) = find_top_level_eq(&s[i..]) {
                    let segment = &s[i..];
                    let key = segment[..eq_offset].trim();
                    let eq_end = i + eq_offset + 1;
                    // output whitespace before key (leading ws in segment)
                    let ws_len = segment.len() - segment.trim_start().len();
                    if ws_len > 0 {
                        push(job, &s[i..i+ws_len], theme.default, &theme.font_id);
                        i += ws_len;
                    }
                    let all_fields: &[&[&str]] = &[TRANSITION_FIELD_NAMES, INTERRUPT_FIELD_NAMES];
                    let known = all_fields.iter().any(|arr| arr.contains(&key));
                    push(job, key, if known { theme.field_known } else { theme.field_unknown }, &theme.font_id);
                    i += key.len();
                    // = and surrounding whitespace
                    push(job, &s[i..eq_end], theme.operator, &theme.font_id);
                    i = eq_end;
                    // skip whitespace after =
                    let ws_start = i;
                    while i < s.len() && s[i..].starts_with(' ') { i += 1; }
                    if i > ws_start {
                        push(job, &s[ws_start..i], theme.default, &theme.font_id);
                    }
                    // value — read until , or } or end
                    let (val, _after) = read_value_token(&s[i..]);
                    colorize_value(job, val, key, theme);
                    i += val.len();
                } else {
                    // unrecognized, emit as default until next , or }
                    let end = s[i..].find([',', '}', '#'])
                        .map(|p| i + p).unwrap_or(s.len());
                    push(job, &s[i..end], theme.default, &theme.font_id);
                    i = end;
                }
            }
        }
    }
}

/// Read a TOML value token (quoted string, number, bool, etc.) stopping at , or } or whitespace at top level
fn read_value_token(s: &str) -> (&str, &str) {
    if s.starts_with('"') {
        // Find closing quote
        let mut i = 1;
        while i < s.len() {
            if s[i..].starts_with('\\') { i += 2; continue; }
            if s[i..].starts_with('"') { i += 1; break; }
            i += 1;
        }
        return (&s[..i], &s[i..]);
    }
    if let Some(after_q) = s.strip_prefix('\'') {
        let end = after_q.find('\'').map(|p| p + 2).unwrap_or(s.len());
        return (&s[..end], &s[end..]);
    }
    // Number/bool/identifier: read until , } ] whitespace newline
    let end = s.find([',', '}', ']', ' ', '\t', '\n', '\r', '#'])
        .unwrap_or(s.len());
    (&s[..end], &s[end..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::FontId;

    fn dark() -> PetstateTheme { PetstateTheme::dark(FontId::monospace(14.0)) }

    #[test]
    fn comment_is_colored_correctly() {
        let t = dark();
        let job = highlight_petstate("# hello world\n", &t);
        assert!(job.sections.iter().any(|s| s.format.color == t.comment));
    }

    #[test]
    fn action_value_is_colored_as_action() {
        let t = dark();
        let job = highlight_petstate("action = \"walk\"\n", &t);
        assert!(job.sections.iter().any(|s| s.format.color == t.action_val),
            "expected action_val color in sections: {:?}", job.sections.iter().map(|s| s.format.color).collect::<Vec<_>>());
    }

    #[test]
    fn unknown_action_value_is_plain_string() {
        let t = dark();
        let job = highlight_petstate("action = \"fly\"\n", &t);
        assert!(job.sections.iter().any(|s| s.format.color == t.string));
        assert!(!job.sections.iter().any(|s| s.format.color == t.action_val));
    }

    #[test]
    fn goto_previous_is_special_keyword() {
        let t = dark();
        let job = highlight_petstate("goto = \"$previous\"\n", &t);
        assert!(job.sections.iter().any(|s| s.format.color == t.special));
    }

    #[test]
    fn duration_value_colored_correctly() {
        let t = dark();
        let job = highlight_petstate("duration = \"500ms\"\n", &t);
        assert!(job.sections.iter().any(|s| s.format.color == t.duration_val));
    }

    #[test]
    fn state_name_in_section_header_is_highlighted() {
        let t = dark();
        let job = highlight_petstate("[states.idle]\n", &t);
        assert!(job.sections.iter().any(|s| s.format.color == t.state_name));
    }

    #[test]
    fn all_action_types_are_recognized() {
        let t = dark();
        for action in ActionType::ALL {
            let code = format!("action = \"{}\"\n", action.as_str());
            let job = highlight_petstate(&code, &t);
            assert!(
                job.sections.iter().any(|s| s.format.color == t.action_val),
                "action \"{}\" not highlighted as action_val", action.as_str()
            );
        }
    }

    #[test]
    fn goto_state_ref_is_colored() {
        let t = dark();
        let job = highlight_petstate("goto = \"idle\"\n", &t);
        assert!(job.sections.iter().any(|s| s.format.color == t.state_ref));
    }

    #[test]
    fn boolean_true_is_colored() {
        let t = dark();
        let job = highlight_petstate("required = true\n", &t);
        assert!(job.sections.iter().any(|s| s.format.color == t.bool_val));
    }

    #[test]
    fn inline_table_in_transitions_array_is_colored() {
        let t = dark();
        // goto in inline table → state_ref; after → duration_val
        let job = highlight_petstate(
            "transitions = [{ goto = \"idle\", after = \"500ms\" }]\n",
            &t,
        );
        assert!(
            job.sections.iter().any(|s| s.format.color == t.state_ref),
            "goto value in inline table should be state_ref color"
        );
        assert!(
            job.sections.iter().any(|s| s.format.color == t.duration_val),
            "after value in inline table should be duration_val color"
        );
    }

    #[test]
    fn in_transitions_resets_on_bracket_with_trailing_comment() {
        let t = dark();
        // The ] line has a trailing comment — in_transitions must still reset
        let code = "transitions = [\n{ goto = \"idle\" },\n] # end\naction = \"walk\"\n";
        let job = highlight_petstate(code, &t);
        // "walk" after the ] should be colored as action_val, not fall through as default
        assert!(
            job.sections.iter().any(|s| s.format.color == t.action_val),
            "action after closing ] with comment should be action_val"
        );
    }
}

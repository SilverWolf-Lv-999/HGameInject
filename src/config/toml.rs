#[derive(Debug, Clone, PartialEq)]
pub enum TomlValue {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Array(Vec<TomlValue>),
    Table(Vec<(String, TomlValue)>),
}

impl TomlValue {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            TomlValue::Bool(flag) => Some(*flag),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            TomlValue::Int(number) => Some(*number),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            TomlValue::Str(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[TomlValue]> {
        match self {
            TomlValue::Array(values) => Some(values),
            _ => None,
        }
    }

    pub fn as_table(&self) -> Option<&[(String, TomlValue)]> {
        match self {
            TomlValue::Table(entries) => Some(entries),
            _ => None,
        }
    }
}

pub fn parse_document(content: &str) -> Vec<(String, TomlValue)> {
    let mut entries = Vec::new();
    let mut section = String::new();
    for raw_line in content.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1]
                .split('.')
                .map(|entry| entry.trim().trim_matches('"'))
                .collect::<Vec<&str>>()
                .join(".");
            continue;
        }
        if let Some(separator) = find_assignment(line) {
            let key = line[..separator].trim().trim_matches('"');
            let value = line[separator + 1..].trim();
            if let Some(parsed) = parse_value(value) {
                if section.is_empty() {
                    entries.push((key.to_owned(), parsed));
                    continue;
                }
                if let Some((module, entry)) = section.split_once('.') {
                    if module == "module" {
                        let name = entry.trim_matches('"').to_owned();
                        if let Some((_, TomlValue::Table(patches))) = entries
                            .iter_mut()
                            .find(|(key, _)| key == "module")
                        {
                            if let Some((_, TomlValue::Table(patch))) = patches
                                .iter_mut()
                                .find(|(key, _)| key == &name)
                            {
                                patch.push((key.to_owned(), parsed));
                            } else {
                                patches.push((
                                    name.clone(),
                                    TomlValue::Table(vec![(key.to_owned(), parsed)]),
                                ));
                            }
                        } else {
                            entries.push((
                                "module".to_owned(),
                                TomlValue::Table(vec![(
                                    name,
                                    TomlValue::Table(vec![(key.to_owned(), parsed)]),
                                )]),
                            ));
                        }
                        continue;
                    }
                }
                entries.push((format!("{section}.{key}"), parsed));
            }
        }
    }
    entries
}

pub fn read_string(content: &str, key: &str) -> Option<String> {
    parse_document(content)
        .into_iter()
        .find(|(name, _)| name == key)
        .and_then(|(_, value)| value.as_str().map(str::to_owned))
}

pub fn serialize_string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            _ => output.push(character),
        }
    }
    output.push('"');
    output
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..index],
            _ => {}
        }
    }
    line
}

fn find_assignment(line: &str) -> Option<usize> {
    let mut quoted = false;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '=' if !quoted => return Some(index),
            _ => {}
        }
    }
    None
}

fn parse_value(text: &str) -> Option<TomlValue> {
    let text = text.trim();
    if text.starts_with('"') {
        return parse_string(text).map(TomlValue::Str);
    }
    if text.starts_with('\'') {
        return parse_literal(text).map(TomlValue::Str);
    }
    if text.starts_with('[') {
        return parse_array(text);
    }
    if text.starts_with('{') {
        return parse_inline_table(text);
    }
    match text {
        "true" => return Some(TomlValue::Bool(true)),
        "false" => return Some(TomlValue::Bool(false)),
        _ => {}
    }
    if let Ok(number) = text.parse::<i64>() {
        return Some(TomlValue::Int(number));
    }
    if let Ok(number) = text.parse::<f64>() {
        return Some(TomlValue::Float(number));
    }
    None
}

fn parse_string(text: &str) -> Option<String> {
    let mut output = String::new();
    let mut escaped = false;
    for character in text[1..].chars() {
        if escaped {
            output.push(match character {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '"' => return Some(output),
            other => output.push(other),
        }
    }
    None
}

fn parse_literal(text: &str) -> Option<String> {
    let end = text[1..].find('\'')?;
    Some(text[1..1 + end].to_owned())
}

fn parse_array(text: &str) -> Option<TomlValue> {
    let end = find_closing(text, 0, '[', ']')?;
    let inner = &text[1..end];
    let mut values = Vec::new();
    for element in split_top_level(inner, ',') {
        let element = element.trim();
        if element.is_empty() {
            continue;
        }
        values.push(parse_value(element)?);
    }
    Some(TomlValue::Array(values))
}

fn parse_inline_table(text: &str) -> Option<TomlValue> {
    let end = find_closing(text, 0, '{', '}')?;
    let inner = &text[1..end];
    let mut entries = Vec::new();
    for element in split_top_level(inner, ',') {
        let element = element.trim();
        if element.is_empty() {
            continue;
        }
        let separator = find_assignment(element)?;
        let key = element[..separator]
            .split('.')
            .map(|entry| entry.trim().trim_matches('"'))
            .collect::<Vec<&str>>()
            .join(".");
        let value = parse_value(element[separator + 1..].trim())?;
        entries.push((key, value));
    }
    Some(TomlValue::Table(entries))
}

fn find_closing(text: &str, start: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0i32;
    let mut quoted = false;
    let mut escaped = false;
    for (index, character) in text.char_indices().skip_while(|(index, _)| *index < start) {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            _ if !quoted => {
                if character == open {
                    depth += 1;
                } else if character == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some(index);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn split_top_level(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut quoted = false;
    let mut escaped = false;
    let mut start = 0usize;
    for (index, character) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '[' | '{' if !quoted => depth += 1,
            ']' | '}' if !quoted => depth -= 1,
            _ if !quoted && character == separator && depth == 0 => {
                parts.push(&text[start..index]);
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

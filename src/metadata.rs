//! Pure Rust metadata models and parsing. No filesystem or hardware access.

use alloc::{
    borrow::ToOwned,
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

const MAX_XML_BYTES: usize = 1024 * 1024;
const MAX_XML_DEPTH: usize = 64;
const MAX_XML_NODES: usize = 16 * 1024;
const MAX_XML_ATTRIBUTES_PER_NODE: usize = 64;
const MAX_XML_TEXT_PER_NODE: usize = 64 * 1024;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AppMetadata {
    pub name: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub short_description: Option<String>,
    pub long_description: Option<String>,
    /// Reborn-normalized signed date key for comparisons; zero epoch is absent.
    pub release_date: Option<i64>,
    pub sort_id: Option<i32>,
    pub arguments: Vec<String>,
    pub ahb_access: bool,
    pub no_ios_reload: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UpdateMetadata {
    pub version: Option<String>,
    pub date: Option<u64>,
    pub notes: Option<String>,
    pub uri: Option<String>,
    pub hash: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserSettings {
    pub device: i32,
    pub sort_order: i32,
    pub browse_mode: i32,
    pub selected_app: Option<String>,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            device: -1,
            sort_order: 0,
            browse_mode: 0,
            selected_app: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ThemeFont {
    pub file: Option<String>,
    pub size: Option<u32>,
    pub color: Option<Rgba>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ThemeMetadata {
    pub description: Option<String>,
    pub languages: Vec<String>,
    pub progress_gradient: Option<[Rgba; 4]>,
    pub default_font: ThemeFont,
    pub fonts: Vec<(Vec<String>, ThemeFont)>,
}

pub fn parse_app_xml(xml: &str) -> Result<AppMetadata, String> {
    let root = parse_xml(xml)?;
    let app = root.find("app").ok_or("missing app element")?;
    let mut author = app.text("author");
    if author.is_none() {
        author = app.text("coder");
    }
    let arguments = app
        .find("arguments")
        .map(|n| n.children_named("arg").filter_map(|a| a.value()).collect())
        .unwrap_or_default();
    Ok(AppMetadata {
        name: app.text("name"),
        author,
        version: app.text("version"),
        short_description: app.text("short_description"),
        long_description: app.text("long_description"),
        release_date: app.text("release_date").and_then(|s| parse_date(&s)),
        sort_id: app.text("sort_id").and_then(|s| parse_sort_id(&s)),
        arguments,
        ahb_access: app.find("ahb_access").is_some(),
        no_ios_reload: app.find("no_ios_reload").is_some(),
    })
}

fn parse_sort_id(value: &str) -> Option<i32> {
    let value = value.trim_start();
    let start = usize::from(value.starts_with(['+', '-']));
    let digits = value[start..]
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    if digits == 0 {
        return None;
    }
    let number = &value[..start + digits];
    Some(number.parse::<i32>().unwrap_or(if number.starts_with('-') {
        i32::MIN
    } else {
        i32::MAX
    }))
}

pub fn parse_update_xml(xml: &str) -> Result<UpdateMetadata, String> {
    let root = parse_xml(xml)?;
    let app = root.find("app").ok_or("missing app element")?;
    Ok(UpdateMetadata {
        version: app.text("version"),
        date: app
            .find("date")
            .and_then(XmlNode::raw_value)
            .and_then(|value| scan_unsigned_decimal(&value)),
        notes: app.text("notes"),
        uri: app.text("uri"),
        hash: app.text("hash"),
    })
}

fn scan_unsigned_decimal(value: &str) -> Option<u64> {
    let bytes = value.as_bytes();
    let mut index = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let negative = match bytes.get(index) {
        Some(b'-') => {
            index += 1;
            true
        }
        Some(b'+') => {
            index += 1;
            false
        }
        _ => false,
    };
    let start = index;
    let mut result = 0u64;
    while let Some(byte @ b'0'..=b'9') = bytes.get(index).copied() {
        result = result
            .checked_mul(10)?
            .checked_add(u64::from(byte - b'0'))?;
        index += 1;
    }
    if index == start {
        return None;
    }
    Some(if negative {
        result.wrapping_neg()
    } else {
        result
    })
}

pub fn parse_settings_xml(xml: &str) -> Result<UserSettings, String> {
    let root = parse_xml(xml)?;
    let node = root.find("hbc").ok_or("missing hbc element")?;
    Ok(UserSettings {
        device: number(node, "device", -1),
        sort_order: number(node, "sort_order", 0),
        browse_mode: number(node, "browse_mode", 0),
        selected_app: node.text("app_sel").map(|s| truncate_utf8_bytes(&s, 63)),
    })
}

pub(crate) fn truncate_utf8_bytes(value: &str, max_bytes: usize) -> String {
    let mut end = value.len().min(max_bytes);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].into()
}

pub fn parse_theme_xml(xml: &str) -> Result<ThemeMetadata, String> {
    let root = parse_xml(xml)?;
    let node = root.find("theme").ok_or("missing theme element")?;
    let mut theme = ThemeMetadata::default();
    if let Some(langs) = node.attrs.get("langs") {
        theme.languages = langs
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| matches!(*s, "ja" | "ko" | "zh"))
            .map(str::to_owned)
            .collect();
        theme.languages.sort();
        theme.languages.dedup();
    }
    theme.description = node
        .text("description")
        .map(|s| truncate_utf8_bytes(&s, 64));
    theme.default_font.color = node.find("font_color").and_then(parse_color);
    theme.progress_gradient = node.find("progress_gradient").and_then(|g| {
        let vals = ["upper_left", "upper_right", "lower_right", "lower_left"];
        let colors: Option<Vec<_>> = vals
            .iter()
            .map(|name| g.find(name).and_then(parse_color))
            .collect();
        colors.and_then(|v| v.try_into().ok())
    });
    for font in node.children_named("font") {
        let targets: Vec<String> = font
            .attrs
            .get("target")
            .map(|v| {
                v.split(|c: char| c == ',' || c.is_whitespace())
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let color = font.find("color").and_then(parse_color);
        let size = font
            .text("size")
            .and_then(|v| v.parse().ok())
            .filter(|v| *v > 0);
        let parsed_font = ThemeFont {
            file: font.text("file"),
            size,
            color,
        };
        if targets.is_empty() {
            // HBC uses an untargeted <font> as the base font. Its values
            // override the legacy <font_color> value field by field.
            if parsed_font.file.is_some() {
                theme.default_font.file = parsed_font.file;
            }
            if parsed_font.size.is_some() {
                theme.default_font.size = parsed_font.size;
            }
            if parsed_font.color.is_some() {
                theme.default_font.color = parsed_font.color;
            }
        } else {
            theme.fonts.push((targets, parsed_font));
        }
    }
    Ok(theme)
}

fn parse_color(node: &XmlNode) -> Option<Rgba> {
    let channel = |name| node.text(name).and_then(|s| s.parse::<u8>().ok());
    Some(Rgba(
        channel("red")?,
        channel("green")?,
        channel("blue")?,
        channel("alpha")?,
    ))
}
fn number(node: &XmlNode, key: &str, fallback: i32) -> i32 {
    let Some(text) = node.find(key).and_then(XmlNode::raw_value) else {
        return fallback;
    };
    scan_atoi(&text).unwrap_or(0)
}

// Reborn uses atoi for settings. It accepts an optional sign and decimal
// prefix, skips ASCII whitespace, and returns zero when no digits follow.
// Checked arithmetic keeps malformed, very large settings safe.
fn scan_atoi(text: &str) -> Option<i32> {
    let bytes = text.as_bytes();
    let mut index = bytes.iter().position(|byte| !byte.is_ascii_whitespace())?;
    let negative = match bytes[index] {
        b'-' => {
            index += 1;
            true
        }
        b'+' => {
            index += 1;
            false
        }
        _ => false,
    };
    let start = index;
    let mut value = 0i64;
    while let Some(byte @ b'0'..=b'9') = bytes.get(index).copied() {
        value = value.checked_mul(10)?.checked_add(i64::from(byte - b'0'))?;
        index += 1;
    }
    if index == start {
        return None;
    }
    let signed = if negative { -value } else { value };
    i32::try_from(signed).ok()
}
fn parse_date(s: &str) -> Option<i64> {
    let mut remaining = s.as_bytes();
    let mut fields = [0i32; 6];
    let mut count = 0;
    for (index, field) in fields.iter_mut().enumerate() {
        let Some(value) = scan_decimal(&mut remaining, if index == 0 { 4 } else { 2 }) else {
            break;
        };
        *field = value;
        count += 1;
    }
    if count < 3 {
        return None;
    }
    // Reborn assigns the parsed month directly to mktime's zero-based tm_mon.
    let mut year = fields[0] + fields[1].div_euclid(12);
    let mut month = fields[1].rem_euclid(12) as u32 + 1;
    let seconds = if count == 6 {
        fields[3] * 3600 + fields[4] * 60 + fields[5]
    } else {
        3600
    };
    let mut day = fields[2] + seconds.div_euclid(86_400);
    let second_of_day = seconds.rem_euclid(86_400) as u32;
    while day <= 0 {
        if month == 1 {
            month = 12;
            year -= 1;
        } else {
            month -= 1;
        }
        day += days_in_month(year, month) as i32;
    }
    loop {
        let days = days_in_month(year, month) as i32;
        if day <= days {
            break;
        }
        day -= days;
        month += 1;
        if month == 13 {
            month = 1;
            year += 1;
        }
    }
    let day = day as u32;
    let hour = second_of_day / 3600;
    let minute = second_of_day / 60 % 60;
    let second = second_of_day % 60;
    // Reborn treats a zero time_t as an absent release date.
    if (year, month, day, second_of_day) == (1970, 1, 1, 0) {
        return None;
    }
    Some(
        i64::from(year) * 10_000_000_000
            + i64::from(month) * 100_000_000
            + i64::from(day) * 1_000_000
            + i64::from(hour) * 10_000
            + i64::from(minute) * 100
            + i64::from(second),
    )
}

fn scan_decimal(input: &mut &[u8], width: usize) -> Option<i32> {
    let trimmed = input.trim_ascii_start();
    let mut length = 0;
    let negative = trimmed.first() == Some(&b'-');
    if matches!(trimmed.first(), Some(b'+' | b'-')) {
        length += 1;
    }
    let digit_start = length;
    let mut value = 0i32;
    while length < width && trimmed.get(length).is_some_and(u8::is_ascii_digit) {
        value = value * 10 + i32::from(trimmed[length] - b'0');
        length += 1;
    }
    if length == digit_start {
        return None;
    }
    *input = &trimmed[length..];
    Some(if negative { -value } else { value })
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if year.rem_euclid(4) == 0
            && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0) =>
        {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[derive(Debug)]
struct XmlNode {
    name: String,
    attrs: BTreeMap<String, String>,
    text: String,
    children: Vec<XmlNode>,
}
impl XmlNode {
    fn find(&self, name: &str) -> Option<&XmlNode> {
        let mut pending = alloc::vec::Vec::new();
        pending.push(self);
        while let Some(node) = pending.pop() {
            if node.name == name {
                return Some(node);
            }
            pending.extend(node.children.iter().rev());
        }
        None
    }
    fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a XmlNode> + 'a {
        self.children.iter().filter(move |n| n.name == name)
    }
    fn text(&self, name: &str) -> Option<String> {
        self.find(name).and_then(XmlNode::value)
    }
    fn value(&self) -> Option<String> {
        let s = self.text.trim();
        (!s.is_empty()).then(|| s.to_owned())
    }
    fn raw_value(&self) -> Option<String> {
        (!self.text.is_empty()).then(|| self.text.clone())
    }
}

// A small, strict XML reader for HBC's metadata vocabulary. Reject malformed
// nesting and decode standard entities, instead of accepting truncated input.
fn parse_xml(input: &str) -> Result<XmlNode, String> {
    if input.len() > MAX_XML_BYTES {
        return Err("XML exceeds size limit".into());
    }
    let mut stack = vec![XmlNode {
        name: String::new(),
        attrs: Default::default(),
        text: String::new(),
        children: vec![],
    }];
    let mut pos = 0;
    let mut node_count = 0usize;
    while pos < input.len() {
        if input[pos..].starts_with("<!--") {
            let end = input[pos + 4..].find("-->").ok_or("unclosed XML comment")?;
            pos += 4 + end + 3;
            continue;
        }
        if input[pos..].starts_with("<?") {
            let end = input[pos + 2..]
                .find("?>")
                .ok_or("unclosed XML declaration")?;
            pos += 2 + end + 2;
            continue;
        }
        if input[pos..].starts_with("<![CDATA[") {
            let end = input[pos + 9..].find("]]>").ok_or("unclosed CDATA")?;
            append_xml_text(&mut stack, &input[pos + 9..pos + 9 + end])?;
            pos += 9 + end + 3;
            continue;
        }
        if input.as_bytes()[pos] != b'<' {
            let end = input[pos..]
                .find('<')
                .map(|n| pos + n)
                .unwrap_or(input.len());
            let decoded = decode(&input[pos..end])?;
            append_xml_text(&mut stack, &decoded)?;
            pos = end;
            continue;
        }
        let end = input[pos..].find('>').ok_or("unclosed XML tag")? + pos;
        let mut raw = input[pos + 1..end].trim();
        pos = end + 1;
        if raw.starts_with('!') {
            return Err("unsupported XML declaration".into());
        }
        if let Some(close) = raw.strip_prefix('/') {
            let name = close.trim();
            if stack.len() <= 1 || stack.last().unwrap().name != name {
                return Err(format!("mismatched closing tag {name}"));
            }
            let node = stack.pop().unwrap();
            stack.last_mut().unwrap().children.push(node);
            continue;
        }
        let self_closing = raw.ends_with('/');
        if self_closing {
            raw = raw[..raw.len() - 1].trim();
        }
        let split = raw.find(char::is_whitespace).unwrap_or(raw.len());
        let name = &raw[..split];
        if name.is_empty() {
            return Err("empty XML tag".into());
        }
        node_count = node_count.checked_add(1).ok_or("too many XML nodes")?;
        if node_count > MAX_XML_NODES {
            return Err("too many XML nodes".into());
        }
        let attrs = parse_attrs(raw[split..].trim())?;
        if attrs.len() > MAX_XML_ATTRIBUTES_PER_NODE {
            return Err("too many XML attributes".into());
        }
        let node = XmlNode {
            name: name.into(),
            attrs,
            text: String::new(),
            children: vec![],
        };
        if self_closing {
            stack.last_mut().unwrap().children.push(node);
        } else {
            if stack.len() > MAX_XML_DEPTH {
                return Err("XML nesting exceeds depth limit".into());
            }
            stack.push(node);
        }
    }
    if stack.len() != 1 {
        return Err(format!("unclosed XML tag {}", stack.last().unwrap().name));
    }
    if stack[0].children.len() != 1 {
        return Err("expected one XML root element".into());
    }
    Ok(stack.remove(0).children.remove(0))
}
fn append_xml_text(stack: &mut [XmlNode], text: &str) -> Result<(), String> {
    let current = stack.last_mut().ok_or("invalid XML parser state")?;
    let new_len = current
        .text
        .len()
        .checked_add(text.len())
        .ok_or("XML text exceeds limit")?;
    if new_len > MAX_XML_TEXT_PER_NODE {
        return Err("XML text exceeds limit".into());
    }
    current.text.push_str(text);
    Ok(())
}
fn parse_attrs(mut s: &str) -> Result<BTreeMap<String, String>, String> {
    let mut map: BTreeMap<String, String> = Default::default();
    let mut count = 0usize;
    while !s.is_empty() {
        s = s.trim_start();
        if s.is_empty() {
            break;
        }
        let eq = s.find('=').ok_or("invalid XML attribute")?;
        let key = s[..eq].trim();
        if key.is_empty() {
            return Err("empty XML attribute name".into());
        }
        s = s[eq + 1..].trim_start();
        let quote = s.chars().next().ok_or("missing XML attribute value")?;
        if quote != '\'' && quote != '"' {
            return Err("unquoted XML attribute".into());
        }
        s = &s[quote.len_utf8()..];
        let n = s.find(quote).ok_or("unclosed XML attribute")?;
        count += 1;
        if count > MAX_XML_ATTRIBUTES_PER_NODE {
            return Err("too many XML attributes".into());
        }
        map.insert(key.into(), decode(&s[..n])?);
        s = &s[n + quote.len_utf8()..];
    }
    Ok(map)
}
fn decode(s: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let e = rest.find(';').ok_or("unclosed XML entity")?;
        let value = match &rest[1..e] {
            "amp" => "&".into(),
            "lt" => "<".into(),
            "gt" => ">".into(),
            "quot" => "\"".into(),
            "apos" => "'".into(),
            v if v.starts_with("#x") => {
                char::from_u32(u32::from_str_radix(&v[2..], 16).map_err(|_| "invalid XML entity")?)
                    .ok_or("invalid XML code point")?
                    .to_string()
            }
            v if v.starts_with('#') => {
                char::from_u32(v[1..].parse().map_err(|_| "invalid XML entity")?)
                    .ok_or("invalid XML code point")?
                    .to_string()
            }
            _ => return Err("unknown XML entity".into()),
        };
        out.push_str(&value);
        rest = &rest[e + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_reborn_sort_id_like_signed_decimal_prefix() {
        for (input, expected) in [
            (" -12tail", Some(-12)),
            ("+7", Some(7)),
            ("x", None),
            ("999999999999", Some(i32::MAX)),
            ("-999999999999", Some(i32::MIN)),
        ] {
            let xml = format!("<app><sort_id>{input}</sort_id></app>");
            assert_eq!(parse_app_xml(&xml).unwrap().sort_id, expected);
        }
    }
    #[test]
    fn parses_app_metadata_entities_dates_arguments_and_flags() {
        let m=parse_app_xml("<app><name>A &amp; B</name><coder>fallback</coder><release_date>20240203112233</release_date><arguments><arg>x</arg><arg>two &lt; three</arg></arguments><no_ios_reload/></app>").unwrap();
        assert_eq!(m.name.as_deref(), Some("A & B"));
        assert_eq!(m.author.as_deref(), Some("fallback"));
        assert_eq!(m.release_date, Some(20240303112233));
        assert_eq!(m.arguments, ["x", "two < three"]);
        assert!(!m.ahb_access);
        assert!(m.no_ios_reload);
    }
    #[test]
    fn parses_settings_update_and_theme_defaults() {
        assert_eq!(
            parse_settings_xml("<hbc><device>2</device><app_sel>").unwrap_err(),
            "unclosed XML tag app_sel"
        );
        let s = parse_settings_xml(
            "<hbc><device>2</device><sort_order>1</sort_order><app_sel>demo</app_sel></hbc>",
        )
        .unwrap();
        assert_eq!(s.selected_app.as_deref(), Some("demo"));
        let u = parse_update_xml(
            "<update><app><version>1</version><date>123</date><hash>x</hash></app></update>",
        )
        .unwrap();
        assert_eq!(u.date, Some(123));
        assert_eq!(u.hash.as_deref(), Some("x"));
        let t=parse_theme_xml("<theme langs='ja, zh'><description>Demo</description><font_color><red>1</red><green>2</green><blue>3</blue><alpha>255</alpha></font_color></theme>").unwrap();
        assert_eq!(t.languages, ["ja", "zh"]);
        assert_eq!(t.default_font.color, Some(Rgba(1, 2, 3, 255)));
        let base = parse_theme_xml("<theme><font_color><red>1</red><green>2</green><blue>3</blue><alpha>255</alpha></font_color><font><file>theme.ttf</file><size>14</size><color><red>9</red><green>8</green><blue>7</blue><alpha>6</alpha></color></font><font target='appname'><size>10</size></font></theme>").unwrap();
        assert_eq!(base.default_font.file.as_deref(), Some("theme.ttf"));
        assert_eq!(base.default_font.size, Some(14));
        assert_eq!(base.default_font.color, Some(Rgba(9, 8, 7, 6)));
        assert_eq!(base.fonts.len(), 1);
        let description = "é".repeat(40);
        let theme = parse_theme_xml(&format!(
            "<theme><description>{description}</description></theme>"
        ))
        .unwrap();
        assert_eq!(theme.description.as_ref().unwrap().len(), 64);
    }

    #[test]
    fn settings_numbers_match_reborn_atoi_prefix_rules() {
        let settings = parse_settings_xml(
            "<hbc><device>  +12tail</device><sort_order>-3.5</sort_order><browse_mode>words</browse_mode></hbc>",
        ).unwrap();
        assert_eq!(
            (settings.device, settings.sort_order, settings.browse_mode),
            (12, -3, 0)
        );
        let missing = parse_settings_xml("<hbc/>").unwrap();
        assert_eq!(
            (missing.device, missing.sort_order, missing.browse_mode),
            (-1, 0, 0)
        );
        let non_ascii_space = parse_settings_xml("<hbc><device>\u{a0}4</device></hbc>").unwrap();
        assert_eq!(non_ascii_space.device, 0);
    }

    #[test]
    fn update_date_matches_reborn_unsigned_scan_rules() {
        let cases = [
            ("123", Some(123)),
            ("  \t123tail", Some(123)),
            ("+456rest", Some(456)),
            ("-7tail", Some(u64::MAX - 6)),
            ("junk", None),
            ("+", None),
            ("\u{a0}123", None),
        ];
        for (input, expected) in cases {
            let xml = format!("<update><app><date>{input}</date></app></update>");
            assert_eq!(parse_update_xml(&xml).unwrap().date, expected, "{input:?}");
        }
        assert_eq!(
            parse_update_xml("<update><app></app></update>")
                .unwrap()
                .date,
            None
        );
    }
    #[test]
    fn release_date_preserves_negative_year_and_zero_epoch_semantics() {
        assert_eq!(parse_date("-0010101"), Some(-9_798_990_000));
        assert_eq!(parse_date("19691201000000"), None);
        assert!(parse_date("-0010101") < parse_date("00000101"));
    }

    #[test]
    fn release_date_normalizes_out_of_range_fields_like_reborn() {
        assert_eq!(parse_date("20241332"), Some(20250304010000));
        assert_eq!(parse_date("20250000"), Some(20241231010000));
        assert_eq!(parse_date("20250101999999"), Some(20250205044039));
        assert_eq!(parse_date("2025-01-02"), Some(20250101010000));
        assert_eq!(parse_date("20250101-1-1-1"), Some(20250131225859));
    }

    #[test]
    fn release_date_uses_reborn_scan_widths_and_partial_time_fallback() {
        let date_only = parse_date("20250101");
        assert_eq!(parse_date("2025010112"), date_only);
        assert_eq!(parse_date("202501011234"), date_only);
        assert_eq!(parse_date("2025 1 1"), date_only);
        assert_eq!(parse_date("2025x0101"), None);
        assert_eq!(
            parse_date("20250101123456tail"),
            parse_date("20250101123456")
        );
    }

    #[test]
    fn release_date_normalizes_reborn_month_and_time_boundaries() {
        assert_eq!(parse_date("20240131"), Some(20240302010000));
        assert_eq!(parse_date("20250131"), Some(20250303010000));
        assert_eq!(parse_date("20241231"), Some(20250131010000));
        assert_eq!(parse_date("20250131235960"), Some(20250304000000));
    }

    #[test]
    fn rejects_bad_xml_and_unscannable_dates() {
        assert!(parse_app_xml("<app><name>x</app>").is_err());
        assert_eq!(parse_date("not a date"), None);
        assert_eq!(parse_date("20250102"), Some(20250202010000));
    }

    #[test]
    fn rejects_xml_that_exceeds_structural_limits() {
        let deep = format!(
            "{}x{}",
            "<a>".repeat(MAX_XML_DEPTH + 1),
            "</a>".repeat(MAX_XML_DEPTH + 1)
        );
        assert_eq!(
            parse_xml(&deep).unwrap_err(),
            "XML nesting exceeds depth limit"
        );

        let many_nodes = format!("<root>{}</root>", "<x/>".repeat(MAX_XML_NODES));
        assert_eq!(parse_xml(&many_nodes).unwrap_err(), "too many XML nodes");

        let too_much_text = format!("<root>{}</root>", "x".repeat(MAX_XML_TEXT_PER_NODE + 1));
        assert_eq!(
            parse_xml(&too_much_text).unwrap_err(),
            "XML text exceeds limit"
        );

        let attributes = (0..=MAX_XML_ATTRIBUTES_PER_NODE)
            .map(|i| format!(" a{i}='v'"))
            .collect::<String>();
        assert_eq!(
            parse_xml(&format!("<root{attributes}/>")).unwrap_err(),
            "too many XML attributes"
        );
        assert_eq!(
            parse_xml(&"x".repeat(MAX_XML_BYTES + 1)).unwrap_err(),
            "XML exceeds size limit"
        );
    }

    #[test]
    fn settings_selection_limit_matches_the_c_byte_buffer() {
        let selected = "é".repeat(40);
        let xml = format!("<hbc><app_sel>{selected}</app_sel></hbc>");
        let settings = parse_settings_xml(&xml).unwrap();
        assert_eq!(settings.selected_app.as_deref().unwrap().len(), 62);
    }
}

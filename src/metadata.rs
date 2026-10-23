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
    /// Numeric `YYYYMMDDhhmmss` form for deterministic comparisons.
    pub release_date: Option<u64>,
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
        arguments,
        ahb_access: app.find("ahb_access").is_some(),
        no_ios_reload: app.find("no_ios_reload").is_some(),
    })
}

pub fn parse_update_xml(xml: &str) -> Result<UpdateMetadata, String> {
    let root = parse_xml(xml)?;
    let app = root.find("app").ok_or("missing app element")?;
    Ok(UpdateMetadata {
        version: app.text("version"),
        date: app.text("date").and_then(|v| v.parse().ok()),
        notes: app.text("notes"),
        uri: app.text("uri"),
        hash: app.text("hash"),
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
    node.text(key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(fallback)
}
fn parse_date(s: &str) -> Option<u64> {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() != 8 && digits.len() != 14 {
        return None;
    }
    let year: u32 = digits[0..4].parse().ok()?;
    let month: u32 = digits[4..6].parse().ok()?;
    let day: u32 = digits[6..8].parse().ok()?;
    if !(1..=12).contains(&month) {
        return None;
    }
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let max_day = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if day == 0 || day > max_day {
        return None;
    }
    if digits.len() == 14 {
        let hour: u32 = digits[8..10].parse().ok()?;
        let minute: u32 = digits[10..12].parse().ok()?;
        let second: u32 = digits[12..14].parse().ok()?;
        if hour > 23 || minute > 59 || second > 60 {
            return None;
        }
    }
    digits.parse().ok()
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
    fn parses_app_metadata_entities_dates_arguments_and_flags() {
        let m=parse_app_xml("<app><name>A &amp; B</name><coder>fallback</coder><release_date>20240203112233</release_date><arguments><arg>x</arg><arg>two &lt; three</arg></arguments><no_ios_reload/></app>").unwrap();
        assert_eq!(m.name.as_deref(), Some("A & B"));
        assert_eq!(m.author.as_deref(), Some("fallback"));
        assert_eq!(m.release_date, Some(20240203112233));
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
    fn rejects_bad_xml_and_invalid_dates() {
        assert!(parse_app_xml("<app><name>x</app>").is_err());
        assert_eq!(parse_date("20241332"), None);
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

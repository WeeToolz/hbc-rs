//! Pure Rust metadata models and parsing. No filesystem or hardware access.

use alloc::{
    borrow::ToOwned,
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

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
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UpdateMetadata {
    pub version: Option<String>,
    pub date: Option<u64>,
    pub notes: Option<String>,
    pub uri: Option<String>,
    pub hash: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UserSettings {
    pub device: i32,
    pub sort_order: i32,
    pub browse_mode: i32,
    pub selected_app: Option<String>,
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
        ahb_access: app.find("ahb_access").is_some() || app.find("no_ios_reload").is_some(),
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
        selected_app: node.text("app_sel").map(|s| s.chars().take(63).collect()),
    })
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
        .map(|s| s.chars().take(64).collect());
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
        let targets = font
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
        theme.fonts.push((
            targets,
            ThemeFont {
                file: font.text("file"),
                size,
                color,
            },
        ));
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
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
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
        if self.name == name {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(name))
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
    let mut stack = vec![XmlNode {
        name: String::new(),
        attrs: Default::default(),
        text: String::new(),
        children: vec![],
    }];
    let mut pos = 0;
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
            stack
                .last_mut()
                .unwrap()
                .text
                .push_str(&input[pos + 9..pos + 9 + end]);
            pos += 9 + end + 3;
            continue;
        }
        if input.as_bytes()[pos] != b'<' {
            let end = input[pos..]
                .find('<')
                .map(|n| pos + n)
                .unwrap_or(input.len());
            stack
                .last_mut()
                .unwrap()
                .text
                .push_str(&decode(&input[pos..end])?);
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
        let attrs = parse_attrs(raw[split..].trim())?;
        let node = XmlNode {
            name: name.into(),
            attrs,
            text: String::new(),
            children: vec![],
        };
        if self_closing {
            stack.last_mut().unwrap().children.push(node);
        } else {
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
fn parse_attrs(mut s: &str) -> Result<BTreeMap<String, String>, String> {
    let mut map: BTreeMap<String, String> = Default::default();
    while !s.is_empty() {
        s = s.trim_start();
        if s.is_empty() {
            break;
        }
        let eq = s.find('=').ok_or("invalid XML attribute")?;
        let key = s[..eq].trim();
        s = s[eq + 1..].trim_start();
        let quote = s.chars().next().ok_or("missing XML attribute value")?;
        if quote != '\'' && quote != '"' {
            return Err("unquoted XML attribute".into());
        }
        s = &s[quote.len_utf8()..];
        let n = s.find(quote).ok_or("unclosed XML attribute")?;
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
        assert!(m.ahb_access);
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
    }
    #[test]
    fn rejects_bad_xml_and_invalid_dates() {
        assert!(parse_app_xml("<app><name>x</app>").is_err());
        assert_eq!(parse_date("20241332"), None);
    }
}

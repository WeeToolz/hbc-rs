use hbc_rs::font::ThemeFontFace;
fn main() {
 let face=ThemeFontFace::parse("droid.ttf".into(),std::fs::read("/Volumes/SSD/WiiDev/WeeToolz/hbc-rs/assets/droid.ttf").unwrap()).unwrap();
 for size in [8,12,16,18,24,32] { for c in 32u8..=126 { println!("size={size} char={c} width={}",face.measure_width(&(c as char).to_string(),size)); } }
}

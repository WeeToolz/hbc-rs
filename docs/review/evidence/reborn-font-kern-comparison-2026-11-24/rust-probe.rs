use hbc_rs::font::ThemeFontFace;
fn main() {
 let face = ThemeFontFace::parse("fixture.ttf".into(),std::fs::read("/private/tmp/hbc-rs/kern-fixture/assets/droid.ttf").unwrap()).unwrap();
 let texts = ["A", "AV", "WWW", "iii", "Hello, world!", "gyp", "M W", "A\nV", "A\n", "A\rV"];
 for size in [8,12,16,18,24,32] { for (index,text) in texts.iter().enumerate() { println!("size={size} index={index} width={}",face.measure_width(text,size)); } println!("kern size={size} value={:?}",face.horizontal_kern('A','V',size)); }
}

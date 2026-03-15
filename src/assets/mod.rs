use rust_embed::Embed;

#[derive(Embed)]
#[folder = "assets/"]
pub struct Assets;

/// Load embedded spritesheet bytes by stem (e.g. "test_pet").
pub fn embedded_sheet(stem: &str) -> Option<(Vec<u8>, Vec<u8>)> {
    let json = Assets::get(&format!("{stem}.json"))?;
    let png = Assets::get(&format!("{stem}.png"))?;
    Some((json.data.into_owned(), png.data.into_owned()))
}

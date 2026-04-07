use wasm_bindgen::prelude::*;
use web_sys::{Blob, BlobPropertyBag, Url};
use js_sys::{Array, Uint8Array};
use ferrite_core::bundle;

pub fn download_bytes(bytes: &[u8], filename: &str, mime: &str) {
    let array = Uint8Array::from(bytes);
    let parts = Array::new();
    parts.push(&array.buffer());
    let mut props = BlobPropertyBag::new();
    props.set_type(mime);
    let blob = Blob::new_with_u8_array_sequence_and_options(&parts, &props)
        .expect("Blob creation failed");
    let url = Url::create_object_url_with_blob(&blob).expect("createObjectURL failed");
    let document = web_sys::window().unwrap().document().unwrap();
    let a = document
        .create_element("a").unwrap()
        .dyn_into::<web_sys::HtmlAnchorElement>().unwrap();
    a.set_href(&url);
    a.set_download(filename);
    a.click();
    Url::revoke_object_url(&url).ok();
}

pub fn import_bundle(data: &[u8]) -> Result<bundle::BundleContents, String> {
    bundle::import(data)
}

pub fn export_bundle(
    bundle_name: &str,
    sprite_json: &str,
    sprite_png: &[u8],
    sm_source: Option<&str>,
) {
    match bundle::export(bundle_name, None, sprite_json, sprite_png, sm_source, None) {
        Ok(bytes) => download_bytes(&bytes, &format!("{bundle_name}.petbundle"), "application/zip"),
        Err(e) => log::error!("bundle export failed: {e}"),
    }
}

pub async fn pick_and_read_bundle() -> Option<Vec<u8>> {
    let handle = rfd::AsyncFileDialog::new()
        .add_filter("Pet Bundle", &["petbundle"])
        .pick_file()
        .await?;
    Some(handle.read().await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn bundle_round_trip() {
        // Create a minimal valid 1x1 PNG
        let mut png = Vec::new();
        {
            let mut encoder = image::codecs::png::PngEncoder::new(&mut png);
            use image::ImageEncoder;
            encoder.write_image(&[255u8, 0, 0, 255], 1, 1, image::ExtendedColorType::Rgba8).unwrap();
        }
        let json = r#"{"frames":[{"frame":{"x":0,"y":0,"w":1,"h":1},"duration":100}],"meta":{"frameTags":[]}}"#;
        let result = ferrite_core::bundle::export("test", None, json, &png, None, None).unwrap();
        let contents = import_bundle(&result).unwrap();
        assert_eq!(contents.bundle_name, "test");
    }
}

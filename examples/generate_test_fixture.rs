//! Generates a test .petbundle fixture for Playwright import/export tests.
//! Usage: cargo run --example generate_test_fixture
fn main() {
    let json_str = std::fs::read_to_string("assets/esheep.json").expect("read JSON");
    let png_bytes = std::fs::read("assets/esheep.png").expect("read PNG");
    let bundle = ferrite_core::bundle::export("esheep", None, &json_str, &png_bytes, None, None)
        .expect("bundle export failed");
    let out = std::path::Path::new("tests/webapp/fixtures/test_bundle.petbundle");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    std::fs::write(out, &bundle).expect("write failed");
    println!("Written {} bytes to {}", bundle.len(), out.display());
}

fn main() {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let include_dir = std::path::Path::new(&crate_dir).join("open-umd/device");

    let result = cbindgen::Builder::new()
        .with_pragma_once(true)
        .with_namespace("luwen")
        .with_crate(crate_dir)
        .generate();

    if let Ok(result) = result {
        // .expect("Unable to generate bindings")
        result.write_to_file(include_dir.join("luwen.h"));
    }
}

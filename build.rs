fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=data.txt");

    let out_dir = std::env::var_os("OUT_DIR").unwrap();
    let path = std::path::Path::new(&out_dir).join("constants.rs");
    std::fs::write(
        &path,
        format!(
            "pub const DEFAULT_CONF_FILE: &str = {:?};",
            std::env::var_os("DEFAULT_CONF_FILE").unwrap_or_else(|| "cf-dynamic.conf".into())
        ),
    )
    .expect("TODO: panic message");
}

fn main() {
    println!("cargo:rerun-if-changed=src/ios_photo_reader.m");
    // The service this build talks to unless a person says otherwise, read at
    // compile time by `nexus_settings`. Declared here because `option_env!`
    // alone does not make cargo rebuild when the value changes, which would
    // hand out a stale service address that nothing explains.
    println!("cargo:rerun-if-env-changed=PORTALIS_NEXUS_DEFAULT_NODE_ID");
    println!("cargo:rerun-if-env-changed=PORTALIS_NEXUS_DEFAULT_ADDR");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("ios") {
        return;
    }

    cc::Build::new()
        .file("src/ios_photo_reader.m")
        .flag("-fobjc-arc")
        .compile("portalis_photo_reader");
    println!("cargo:rustc-link-lib=framework=Photos");
    println!("cargo:rustc-link-lib=framework=AVFoundation");
    println!("cargo:rustc-link-lib=framework=Foundation");
}

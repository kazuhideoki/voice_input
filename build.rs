fn main() {
    // macOSでnative-musicフィーチャーが有効な場合のみswift-bridgeをビルド
    // 現在は段階的移行のため無効化
    /*
    #[cfg(all(target_os = "macos", feature = "native-music"))]
    {
        if let Ok(out_dir) = std::env::var("OUT_DIR") {
            // swift-bridgeの設定
            swift_bridge_build::parse_bridges(vec!["src/native/mod.rs"])
                .write_all_concatenated(
                    out_dir,
                    env!("CARGO_PKG_NAME")
                );
        }
    }
    */
    
    // 現在は何もしない（段階的移行中）
    println!("cargo:rerun-if-changed=build.rs");
}
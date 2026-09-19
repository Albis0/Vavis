fn main() {
    // Rebuild when the interface bundle changes.
    //
    // `tauri_build` embeds `ui/dist` but never builds it, and `cargo build`
    // does not run `beforeBuildCommand` -- that belongs to `tauri build`.
    // So a `cargo build --release` after an interface change happily
    // reuses whatever `dist` already held, and the new exe ships the old
    // screen. That cost a round of "the fix did not work" once: the CSS was
    // correct, the bundle was from an hour earlier.
    //
    // This does not build the interface -- nothing here can. It makes cargo
    // notice a newer bundle and relink, so `npm run build` followed by
    // `cargo build` is enough, and a forgotten `npm run build` shows up as
    // an exe that is genuinely unchanged rather than one that silently
    // disagrees with the source.
    println!("cargo:rerun-if-changed=../../ui/dist");

    tauri_build::build();
}

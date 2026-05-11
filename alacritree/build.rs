// Embeds the application icon into the Windows .exe so File Explorer, the
// taskbar, and Scoop-generated shortcuts all show the proper icon instead of
// the default executable glyph.
fn main() {
    #[cfg(windows)]
    embed_resource::compile("./windows/alacritree.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}

#![cfg(all(target_os = "windows", feature = "backend-windows"))]

#[test]
fn windows_backend_embeds_activation_manifest_without_extra_features() {
    let manifest = include_str!(concat!(env!("OUT_DIR"), "/app.manifest"));
    assert_eq!(manifest.matches("<winrtv1:activatableClass ").count(), 920);
    for class in [
        "Microsoft.UI.Xaml.Application",
        "Microsoft.UI.Xaml.Window",
        "Microsoft.UI.Xaml.Controls.XamlControlsResources",
    ] {
        assert!(manifest.contains(&format!("name=\"{class}\"")));
    }
    assert!(!manifest.contains("<!-- ACTIVATION_CLASSES -->"));
}

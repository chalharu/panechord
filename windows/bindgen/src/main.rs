//! Generate SDK bindings from pinned metadata using official windows-bindgen.
//! The upstream clone is read-only; no private Rust binding source is copied.
use std::{fs, path::Path, process::Command};

const REVISION: &str = "a57cc321bfdbac6bb98c488ba49f399da8fcb98a";
const SOURCE: &str = "crates/tools/reactor/winmd";
const FILTERS: &[&str] = &[
    "Microsoft.UI.Xaml.Application",
    "Microsoft.UI.Xaml.Application.CreateInstance",
    "Microsoft.UI.Xaml.Application.Current",
    "Microsoft.UI.Xaml.Application.Start",
    "Microsoft.UI.Xaml.IApplication",
    "Microsoft.UI.Xaml.IApplicationOverrides",
    "Microsoft.UI.Xaml.Markup.IXamlMetadataProvider",
    "Microsoft.UI.Xaml.Markup.IXamlType::{}",
    "Microsoft.UI.Xaml.Markup.XmlnsDefinition",
    "Windows.UI.Xaml.Interop.TypeName",
    "extras.IWindowNative",
    "Microsoft.UI.Xaml.Window",
    "Microsoft.UI.Xaml.Window.CreateInstance",
    "Microsoft.UI.Xaml.IWindow",
    "Microsoft.UI.Xaml.IFrameworkElement",
    "Microsoft.UI.Xaml.IFrameworkElementOverrides",
    "Microsoft.UI.Xaml.IUIElement",
    "Microsoft.UI.Xaml.Controls.Panel",
    "Microsoft.UI.Xaml.Controls.Panel.CreateInstance",
    "Microsoft.UI.Xaml.Controls.StackPanel",
    "Microsoft.UI.Xaml.Controls.StackPanel.CreateInstance",
    "Microsoft.UI.Xaml.Controls.Grid",
    "Microsoft.UI.Xaml.Controls.Grid.CreateInstance",
    "Microsoft.UI.Xaml.Controls.TextBlock",
    "Microsoft.UI.Xaml.Controls.TextBlock.CreateInstance",
    "Microsoft.UI.Xaml.Controls.ITextBlock",
    "Microsoft.UI.Xaml.Controls.TextBox",
    "Microsoft.UI.Xaml.Controls.TextBox.CreateInstance",
    "Microsoft.UI.Xaml.Controls.ITextBox",
    "Microsoft.UI.Xaml.Controls.Button",
    "Microsoft.UI.Xaml.Controls.Button.CreateInstance",
    "Microsoft.UI.Xaml.Controls.IContentControl",
    "Microsoft.UI.Xaml.Controls.IPanel",
    "Microsoft.UI.Xaml.Controls.IStackPanel",
    "Microsoft.UI.Xaml.Controls.IGrid",
    "Microsoft.UI.Xaml.Controls.Grid.SetRow",
    "Microsoft.UI.Xaml.Controls.Grid.SetColumn",
    "Microsoft.UI.Xaml.Controls.Grid.SetRowSpan",
    "Microsoft.UI.Xaml.Controls.Grid.SetColumnSpan",
    "Microsoft.UI.Xaml.Controls.Grid.GetRow",
    "Microsoft.UI.Xaml.Controls.Grid.GetColumn",
    "Microsoft.UI.Xaml.Controls.Grid.GetRowSpan",
    "Microsoft.UI.Xaml.Controls.Grid.GetColumnSpan",
    "Microsoft.UI.Xaml.Controls.Canvas",
    "Microsoft.UI.Xaml.Controls.Canvas.CreateInstance",
    "Microsoft.UI.Xaml.Controls.Canvas.SetLeft",
    "Microsoft.UI.Xaml.Controls.Canvas.SetTop",
    "Microsoft.UI.Xaml.Controls.Canvas.GetLeft",
    "Microsoft.UI.Xaml.Controls.Canvas.GetTop",
    "Microsoft.UI.Xaml.Controls.Border",
    "Microsoft.UI.Xaml.Controls.Border.CreateInstance",
    "Microsoft.UI.Xaml.Controls.IBorder",
    "Microsoft.UI.Xaml.Controls.IControl",
    "Microsoft.UI.Xaml.Controls.Primitives.IButtonBase",
    "Microsoft.UI.Xaml.Controls.Primitives.IToggleButton",
    "Microsoft.UI.Xaml.Controls.Primitives.IRangeBase",
    "Microsoft.UI.Xaml.Controls.Primitives.ISelector",
    "Microsoft.UI.Xaml.Controls.IItemsControl",
    "Microsoft.UI.Xaml.Controls.ScrollViewer",
    "Microsoft.UI.Xaml.Controls.ScrollViewer.CreateInstance",
    "Microsoft.UI.Xaml.Controls.IScrollViewer",
    "Microsoft.UI.Xaml.Controls.CheckBox",
    "Microsoft.UI.Xaml.Controls.CheckBox.CreateInstance",
    "Microsoft.UI.Xaml.Controls.RadioButton",
    "Microsoft.UI.Xaml.Controls.RadioButton.CreateInstance",
    "Microsoft.UI.Xaml.Controls.IRadioButton",
    "Microsoft.UI.Xaml.Controls.Slider",
    "Microsoft.UI.Xaml.Controls.Slider.CreateInstance",
    "Microsoft.UI.Xaml.Controls.ISlider",
    "Microsoft.UI.Xaml.Controls.ComboBox",
    "Microsoft.UI.Xaml.Controls.ComboBox.CreateInstance",
    "Microsoft.UI.Xaml.Controls.IComboBox",
    "Microsoft.UI.Xaml.Controls.Image",
    "Microsoft.UI.Xaml.Controls.Image.CreateInstance",
    "Microsoft.UI.Xaml.Controls.IImage",
    "Microsoft.UI.Xaml.Controls.ProgressBar",
    "Microsoft.UI.Xaml.Controls.ProgressBar.CreateInstance",
    "Microsoft.UI.Xaml.Controls.IProgressBar",
    "Microsoft.UI.Xaml.Controls.RowDefinition",
    "Microsoft.UI.Xaml.Controls.RowDefinition.CreateInstance",
    "Microsoft.UI.Xaml.Controls.IRowDefinition",
    "Microsoft.UI.Xaml.Controls.ColumnDefinition",
    "Microsoft.UI.Xaml.Controls.ColumnDefinition.CreateInstance",
    "Microsoft.UI.Xaml.Controls.IColumnDefinition",
    "Microsoft.UI.Xaml.Controls.ContentDialog",
    "Microsoft.UI.Xaml.Controls.ContentDialog.CreateInstance",
    "Microsoft.UI.Xaml.Controls.IContentDialog",
    "Microsoft.UI.Xaml.Controls.MenuBar",
    "Microsoft.UI.Xaml.Controls.MenuBar.CreateInstance",
    "Microsoft.UI.Xaml.Controls.IMenuBar",
    "Microsoft.UI.Xaml.Controls.MenuBarItem",
    "Microsoft.UI.Xaml.Controls.MenuBarItem.CreateInstance",
    "Microsoft.UI.Xaml.Controls.IMenuBarItem",
    "Microsoft.UI.Xaml.Controls.MenuFlyoutItem",
    "Microsoft.UI.Xaml.Controls.MenuFlyoutItem.CreateInstance",
    "Microsoft.UI.Xaml.Controls.IMenuFlyoutItem",
    "Microsoft.UI.Xaml.Input.KeyboardAccelerator",
    "Microsoft.UI.Xaml.Input.KeyboardAccelerator.CreateInstance",
    "Microsoft.UI.Xaml.Input.IKeyboardAccelerator",
    "Microsoft.UI.Xaml.Input.IKeyboardAcceleratorInvokedEventArgs",
    "Microsoft.UI.Xaml.Controls.MenuFlyoutSubItem",
    "Microsoft.UI.Xaml.Controls.MenuFlyoutSubItem.CreateInstance",
    "Microsoft.UI.Xaml.Controls.IMenuFlyoutSubItem",
    "Microsoft.UI.Xaml.Controls.MenuFlyoutSeparator",
    "Microsoft.UI.Xaml.Controls.MenuFlyoutSeparator.CreateInstance",
    "Microsoft.UI.Xaml.Media.SolidColorBrush",
    "Microsoft.UI.Xaml.Media.SolidColorBrush.CreateInstance",
    "Microsoft.UI.Xaml.Media.ISolidColorBrush",
    "Microsoft.UI.Xaml.Media.Imaging.BitmapImage",
    "Microsoft.UI.Xaml.Media.Imaging.BitmapImage.CreateInstance",
    "Microsoft.UI.Xaml.Media.Imaging.IBitmapImage",
    "Microsoft.UI.Xaml.Media.Imaging.IBitmapSource",
    "Windows.Storage.Streams.InMemoryRandomAccessStream",
    "Windows.Storage.Streams.InMemoryRandomAccessStream.CreateInstance",
    "Windows.Storage.Streams.DataWriter",
    "Windows.Storage.Streams.DataWriter.CreateDataWriter",
    "Windows.Storage.Streams.IDataWriter",
    "Windows.Storage.Streams.IRandomAccessStream",
    "Microsoft.UI.Xaml.Input.IPointerRoutedEventArgs",
    "Microsoft.UI.Input.IPointerPoint",
    "Microsoft.UI.Input.IPointerPointProperties",
    "Microsoft.UI.Xaml.Controls.IContentDialogClosedEventArgs",
    "Microsoft.UI.Xaml.Controls.Primitives.IRangeBaseValueChangedEventArgs",
    "Microsoft.UI.Xaml.IDependencyObject",
    "Windows.UI.Text.FontWeight",
    "Windows.Win32.GetDpiForWindow",
    "Windows.Win32.AdjustWindowRectExForDpi",
    "Microsoft.UI.Xaml.IResourceDictionary",
    "Microsoft.UI.Xaml.ResourceDictionary.CreateInstance",
    "Microsoft.UI.Xaml.Controls.XamlControlsResources",
    "Microsoft.UI.Xaml.Controls.XamlControlsResources.CreateInstance",
    "Microsoft.UI.Xaml.XamlTypeInfo.XamlControlsXamlMetaDataProvider",
    "Microsoft.UI.Xaml.XamlTypeInfo.XamlControlsXamlMetaDataProvider.CreateInstance",
    "Microsoft.UI.Dispatching.DispatcherQueue",
    "Microsoft.UI.Dispatching.IDispatcherQueue",
    "Microsoft.UI.Dispatching.DispatcherQueueTimer",
    "Microsoft.UI.Dispatching.IDispatcherQueueTimer",
    "Windows.Win32.CoInitializeEx",
    "Windows.Win32.CoUninitialize",
    "Windows.Win32.COINIT_APARTMENTTHREADED",
];

fn git(clone: &Path, args: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .arg("-C")
        .arg(clone)
        .args(args)
        .output()
        .expect("cannot read SDK metadata from git");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    output.stdout
}

fn main() {
    let clone = std::env::args_os().nth(1).expect("supply the read-only windows-rs clone path");
    let clone = Path::new(&clone);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let inputs = root.join("bindgen/inputs");
    fs::create_dir_all(&inputs).expect("cannot create metadata directory");
    fs::create_dir_all(root.join("src")).expect("cannot create output directory");
    assert_eq!(FILTERS.len(), 140, "unexpected filter inventory");

    let tool = git(clone, &["show", &format!("{REVISION}:crates/tools/reactor/src/main.rs")]);
    assert!(String::from_utf8_lossy(&tool).contains("const WINDOWS_APP_SDK_VERSION: &str = \"2.4.0\";"),
        "metadata revision must match Runtime 2.4.0");
    let listing = git(clone, &["ls-tree", "-r", "--name-only", REVISION, SOURCE]);
    let listing = String::from_utf8(listing).expect("invalid metadata paths");
    let paths: Vec<_> = listing.lines().filter(|path| path.ends_with(".winmd")).collect();
    assert_eq!(paths.len(), 27, "unexpected published SDK metadata inventory");
    // Never let an extra locally supplied WinMD silently affect regeneration.
    for entry in fs::read_dir(&inputs).expect("cannot inspect metadata directory") {
        let entry = entry.expect("cannot inspect metadata input");
        let name = entry.file_name();
        if entry.path().extension().is_some_and(|extension| extension == "winmd") {
            assert!(paths.iter().any(|path| Path::new(path).file_name() == Some(name.as_os_str())),
                "unlisted metadata input: {}", entry.path().display());
        }
    }
    // Each recorded identifier is a Git blob object ID (SHA-1), not SHA-256.
    let mut provenance = format!("generator=windows-bindgen 0.100.0\nsdk=2.4.0\nrevision={REVISION}\n");
    for path in paths {
        let name = Path::new(path).file_name().expect("metadata filename missing");
        assert_eq!(Path::new(path).parent(), Some(Path::new(SOURCE)), "unexpected metadata path");
        let object = format!("{REVISION}:{path}");
        let bytes = git(clone, &["show", &object]);
        assert!(bytes.starts_with(b"MZ"), "invalid WinMD PE: {path}");
        let blob = git(clone, &["rev-parse", &object]);
        provenance.push_str(&format!("{} {}\n", name.to_string_lossy(), String::from_utf8_lossy(&blob).trim()));
        fs::write(inputs.join(name), bytes).expect("cannot write metadata input");
    }
    fs::write(inputs.join("PROVENANCE.txt"), provenance).expect("cannot write provenance");
    windows_bindgen::builder()
        .input(&inputs)
        .input_default()
        .output(root.join("src/owned_bindings.rs"))
        .implements([
            "Microsoft.UI.Xaml.IApplicationOverrides",
            "Microsoft.UI.Xaml.IFrameworkElementOverrides",
            "Microsoft.UI.Xaml.Markup.IXamlMetadataProvider",
        ])
        .compose("Microsoft.UI.Xaml.Application")
        .compose("Microsoft.UI.Xaml.Controls.Panel")
        .minimal()
        .dead_code()
        .flat()
        .filters(FILTERS.iter().copied())
        .write();
    println!("SDK_BINDINGS_GENERATED revision={REVISION} sdk=2.4.0 inputs=27 filters={}", FILTERS.len());
}
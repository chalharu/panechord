// Test-only layout bridge. Compiled into libPanechordAppKit.dylib only when
// PANECHORD_TEST_BRIDGE=1 (see macos/build.rs); release/distribution builds
// never contain it. Same module as AppKit.swift, so `host`/`string` and the
// container classes resolve without extra FFI.
@_cdecl("w3_dump_layout") @MainActor
public func dumpLayout(_ pointer: UnsafeMutableRawPointer,
          _ output: UnsafeMutablePointer<UInt8>?, _ capacity: Int) -> Int {
    let value = host(pointer)
    value.window.contentView?.layoutSubtreeIfNeeded()
    guard let content = value.window.contentView else { return 0 }
    // Same field order as the GTK backend (`id,kind,x,y,width,height`)
    // so goldens stay diffable across backends.
    var parts: [String] = []
    for id in value.frames.keys.sorted() {
        guard let frame = value.frames[id], frame.superview != nil, !frame.isHidden else { continue }
        let rect = frame.convert(frame.bounds, to: content)
        let node = value.nodes[id]
        let role: String
        if node is NSTextField {
            role = (node as? NSTextField)?.isEditable == true ? "entry" : "label"
        } else if node is NSButton {
            role = "button"
        } else if node is NSSlider {
            role = "slider"
        } else if node is NSPopUpButton {
            role = "combo"
        } else if node is Tabs {
            role = "tab"
        } else if node is Rows {
            role = "list"
        } else if node is Scrolling {
            role = "scroll"
        } else if node is GridContainer {
            role = "grid"
        } else if node is FlexContainer {
            role = "flex"
        } else if node is AbsoluteContainer {
            role = "absolute"
        } else if node is PointerImage {
            role = "image"
        } else if node is NSProgressIndicator {
            role = "meter"
        } else if node is NSBox {
            role = "separator"
        } else if node is NSStackView {
            role = "stack"
        } else {
            role = "widget"
        }
        let safe = id
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
        let x = Int(rect.origin.x.rounded()), y = Int(rect.origin.y.rounded())
        let w = Int(rect.size.width.rounded()), h = Int(rect.size.height.rounded())
        parts.append("{\"id\":\"\(safe)\",\"kind\":\"\(role)\",\"x\":\(x),\"y\":\(y),\"width\":\(w),\"height\":\(h)}")
    }
    let json = "[" + parts.joined(separator: ",") + "]"
    guard let data = json.data(using: .utf8) else { return 0 }
    if let output = output, capacity >= data.count {
        data.withUnsafeBytes { source in
            if let base = source.baseAddress { output.update(from: base.assumingMemoryBound(to: UInt8.self), count: data.count) }
        }
    }
    return data.count
}

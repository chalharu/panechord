// Test-only layout bridge. Compiled into libPanechordAppKit.dylib only when
// `test-bridge` feature (see macos/build.rs); release/distribution builds
// never contain it. Same module as AppKit.swift, so `host`/`string` and the
// container classes resolve without extra FFI.
import AppKit

@_cdecl("w3_dump_layout") @MainActor
public func dumpLayout(_ pointer: UnsafeMutableRawPointer,
          _ output: UnsafeMutablePointer<UInt8>?, _ capacity: Int) -> Int {
    let value = host(pointer)
    value.window.contentView?.layoutSubtreeIfNeeded()
    guard let content = value.window.contentView else { return 0 }
    // Same field order as the GTK backend (`id,kind,x,y,width,height`)
    // so goldens stay diffable across backends. `__root` is excluded: its
    // geometry is backend-defined window chrome (content-sized here, window
    // fill on GTK), so only the corpus itself is cross-backend comparable.
    var parts: [String] = []
    for id in value.frames.keys.sorted() {
        guard id != "__root" else { continue }
        guard let frame = value.frames[id], frame.superview != nil, !frame.isHidden else { continue }
        let rect = frame.convert(frame.bounds, to: content)
        // Declared kind from w3_add/w3_add_grid (`value.kinds`): never sniff
        // AppKit classes, so subclassing and edit state cannot mislabel.
        // -1 is the grid container (separate w3_add_grid entry point).
        let role: String
        switch value.kinds[id] ?? -100 {
        case 0, 1, 11, 12: role = "stack"
        case 2: role = "label"
        case 3: role = "entry"
        case 4: role = "button"
        case 6, 7: role = "check"
        case 8: role = "slider"
        case 9: role = "combo"
        case 10: role = "tab"
        case 13, 23: role = "image"
        case 15: role = "scroll"
        case 16: role = "separator"
        case 17: role = "stack" // GTK mounts tables in a plain Box; keep kinds comparable
        case 18: role = "meter"
        case 21: role = "flex"
        case 22: role = "absolute"
        case -1: role = "grid"
        default: role = "widget"
        }
        let safe = id
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
        let x = Int(rect.origin.x.rounded())
        let y = Int(rect.origin.y.rounded())
        let w = Int(rect.size.width.rounded())
        let h = Int(rect.size.height.rounded())
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

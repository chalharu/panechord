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
        // Declared role from w3_add/w3_add_grid: never sniff AppKit classes
        // (subclassing and edit state would mislabel controls).
        let role = value.roles[id] ?? "widget"
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

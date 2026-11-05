import AppKit

public typealias Callback = @convention(c) (UnsafeMutableRawPointer?, Int32, UnsafePointer<UInt8>?, Int, UnsafePointer<UInt8>?, Int) -> Void

func string(_ bytes: UnsafePointer<UInt8>?, _ count: Int) -> String {
    guard let bytes = bytes else { return "" }
    return String(decoding: UnsafeBufferPointer(start: bytes, count: count), as: UTF8.self)
}

struct TableDefinition: Decodable {
    struct Column: Decodable {
        let title: String
        let width: Double?
        let role: Int
        let scale: Double
        let foreground: [Int]?
        let overflow: Int
        let stretch: Bool
        let hideEmpty: Bool
        var font: NSFont {
            let size = NSFont.systemFontSize * scale
            switch role {
            case 2: return NSFont.monospacedSystemFont(ofSize: size, weight: .regular)
            case 1: return NSFont.boldSystemFont(ofSize: size * 1.2)
            default: return NSFont.systemFont(ofSize: size)
            }
        }
    }
    struct Cell: Decodable { let text: String; let foreground: [Int]? }
    struct Row: Decodable { let key: String; let cells: [Cell] }
    let columns: [Column]
    let rows: [Row]
    let emptyText: String
    let columnGap: Double
    let rowGap: Double
    let rowInset: Double
}

@MainActor
final class SelectableTableCell: NSTableCellView {
    weak var owner: TableControl?
    var rowKey = ""
    var column = 0
    var textInsets: [NSLayoutConstraint] = []
    override func mouseDown(with event: NSEvent) {
        if owner?.selectionPresent == true { owner?.pick(self) }
        else { super.mouseDown(with: event) }
    }
}

struct TableCursor: Decodable {
    let rowKey: String
    let column: Int

    enum CodingKeys: String, CodingKey {
        case rowKey = "row_key"
        case column
    }
}

@MainActor
final class TableControl: NSScrollView, NSTableViewDataSource, NSTableViewDelegate {
    let table = NSTableView()
    let empty = NSTextField(labelWithString: "")
    var definition: TableDefinition?
    weak var owner: Host?
    var key = ""
    var cursor: TableCursor?
    var selectionPresent = false
    var selectionEnabled = false
    /// Fill the window viewport when no explicit height is styled (history):
    /// gravity sizes scroll views by fitting (zero), collapsing the grid.
    var fillsViewport = false
    var floorConstraint: NSLayoutConstraint?

    func setCursor(_ text: String) {
        guard let data = text.data(using: .utf8), let value = try? JSONDecoder().decode(TableCursor?.self, from: data) else {
            if text == "null" { cursor = nil; table.reloadData() }
            return
        }
        cursor = value
        table.reloadData()
    }

    func pick(_ cell: SelectableTableCell) {
        guard selectionEnabled, let owner, !owner.closed, owner.interactive(key), owner.nodes[key] === self,
              table.row(for: cell) >= 0, let definition,
              definition.columns.indices.contains(cell.column), definition.rows.contains(where: { $0.key == cell.rowKey }),
              let data = try? JSONSerialization.data(withJSONObject: [cell.rowKey, cell.column]),
              let value = String(data: data, encoding: .utf8) else { return }
        owner.emit(key, value)
    }

    init() {
        super.init(frame: NSRect(x: 0, y: 0, width: 480, height: 264))
        hasVerticalScroller = true
        scrollerStyle = .overlay
        autohidesScrollers = true
        drawsBackground = false
        documentView = table
        table.dataSource = self
        table.delegate = self
        table.allowsEmptySelection = true
        table.allowsColumnReordering = false
        table.columnAutoresizingStyle = .lastColumnOnlyAutoresizingStyle
        contentView.addSubview(empty)
        empty.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([empty.centerXAnchor.constraint(equalTo: contentView.centerXAnchor),
            empty.centerYAnchor.constraint(equalTo: contentView.centerYAnchor)])
    }
    required init?(coder _: NSCoder) { fatalError("init(coder:) is unavailable") }

    /// Column width from content: explicit width wins, otherwise fit
    /// the widest cell of that column. Stretch columns keep a 120pt
    /// floor so empty tables do not collapse (capped against runaway
    /// user text); jagged rows measure their own cells only.
    static func fittedWidth(
        _ value: TableDefinition.Column,
        column: Int,
        rows: [TableDefinition.Row],
        headerFont: NSFont
    ) -> CGFloat {
        if let width = value.width {
            return CGFloat(width)
        }
        var natural = (value.title as NSString).size(withAttributes: [.font: headerFont]).width
        for row in rows {
            if column < row.cells.count {
                natural = max(
                    natural,
                    (row.cells[column].text as NSString).size(withAttributes: [.font: value.font]).width)
            }
        }
        return value.stretch ? min(800, max(120, natural + 4)) : min(400, natural + 4)
    }

    func update(_ text: String) {
        guard let bytes = text.data(using: .utf8), let next = try? JSONDecoder().decode(TableDefinition.self, from: bytes) else { return }
        let origin = contentView.bounds.origin
        let columnsChanged = definition?.columns.map { $0.title } != next.columns.map { $0.title }
            || definition?.columns.map { $0.width } != next.columns.map { $0.width }
        // Widths depend on row content, not just the schema: the first
        // payload carries an empty projection (headers only), so measuring
        // once would pin every column to the minimum forever. Refit when
        // the row set changes; value-only updates keep their widths.
        let rowKeysChanged = definition?.rows.map(\.key) != next.rows.map(\.key)
        definition = next
        let headers = next.columns.contains { !$0.title.isEmpty }
        if headers != (table.headerView != nil) {
            table.headerView = headers ? NSTableHeaderView(frame: NSRect(x: 0, y: 0, width: 480, height: 24)) : nil
        }
        empty.stringValue = next.emptyText
        empty.isHidden = !next.rows.isEmpty
        if columnsChanged {
            for column in table.tableColumns { table.removeTableColumn(column) }
            for (index, value) in next.columns.enumerated() {
                let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier(String(index)))
                column.title = value.title
                column.headerCell.font = NSFont.boldSystemFont(ofSize: NSFont.systemFontSize)
                column.width = TableControl.fittedWidth(
                    value,
                    column: index,
                    rows: next.rows,
                    headerFont: column.headerCell.font ?? NSFont.systemFont(ofSize: NSFont.systemFontSize))
                column.minWidth = value.width ?? 24
                column.resizingMask = value.stretch ? .autoresizingMask : []
                table.addTableColumn(column)
            }
        } else if rowKeysChanged {
            // Same schema, new rows: refit the live columns in place
            // (no teardown) so populated content escapes the minimum.
            for (live, value) in zip(table.tableColumns, next.columns) {
                live.width = TableControl.fittedWidth(
                    value,
                    column: Int(live.identifier.rawValue) ?? 0,
                    rows: next.rows,
                    headerFont: live.headerCell.font ?? NSFont.systemFont(ofSize: NSFont.systemFontSize))
            }
        }
        table.intercellSpacing = NSSize(width: next.columnGap, height: next.rowGap)
        table.reloadData()
        contentView.scroll(to: origin)
        reflectScrolledClipView(contentView)
    }
    func numberOfRows(in _: NSTableView) -> Int { definition?.rows.count ?? 0 }
    func tableView(_ _: NSTableView, shouldSelectRow _: Int) -> Bool { false }
    func tableView(_ tableView: NSTableView, heightOfRow row: Int) -> CGFloat {
        guard let definition = definition else { return 16 }
        let height = definition.columns.enumerated().reduce(CGFloat(selectionPresent ? 34 : 16)) { height, entry in
            let (index, column) = entry
            guard column.overflow == 1 else { return max(height, column.font.ascender - column.font.descender) }
            let width = max(1, tableView.tableColumns[index].width - 4)
            let measured = (definition.rows[row].cells[index].text as NSString).boundingRect(
                with: NSSize(width: width, height: .greatestFiniteMagnitude), options: [.usesLineFragmentOrigin], attributes: [.font: column.font])
            return max(height, ceil(measured.height))
        }
        return height + definition.rowInset * 2
    }
    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        guard let definition = definition, let tableColumn = tableColumn,
            let index = Int(tableColumn.identifier.rawValue) else { return nil }
        let column = definition.columns[index]
        let value = definition.rows[row].cells[index]
        let cell = (tableView.makeView(withIdentifier: tableColumn.identifier, owner: self) as? SelectableTableCell) ?? SelectableTableCell()
        cell.owner = self
        cell.rowKey = definition.rows[row].key
        cell.column = index
        cell.isHidden = column.hideEmpty && value.text.isEmpty
        cell.identifier = tableColumn.identifier
        cell.setAccessibilityIdentifier(definition.rows[row].key + "/" + String(index))
        let field = cell.textField ?? NSTextField(wrappingLabelWithString: "")
        if cell.textField == nil {
            cell.textField = field
            cell.addSubview(field)
            field.translatesAutoresizingMaskIntoConstraints = false
            cell.textInsets = [field.leadingAnchor.constraint(equalTo: cell.leadingAnchor),
                field.trailingAnchor.constraint(equalTo: cell.trailingAnchor), field.topAnchor.constraint(equalTo: cell.topAnchor)]
            NSLayoutConstraint.activate(cell.textInsets)
        }
        for (index, constraint) in cell.textInsets.enumerated() {
            constraint.constant = index == 1 ? -definition.rowInset : definition.rowInset
        }
        field.stringValue = value.text
        field.isSelectable = !selectionPresent
        field.font = column.font
        field.maximumNumberOfLines = column.overflow == 1 ? 0 : 1
        switch column.overflow {
        case 2: field.lineBreakMode = .byTruncatingMiddle
        case 1: field.lineBreakMode = .byWordWrapping
        default: field.lineBreakMode = .byClipping
        }
        let rgb = value.foreground ?? column.foreground
        let active = cursor?.rowKey == cell.rowKey && cursor?.column == index
        cell.wantsLayer = true
        cell.layer?.backgroundColor = active ? NSColor.selectedContentBackgroundColor.cgColor : NSColor.clear.cgColor
        field.textColor = active ? .selectedTextColor : rgb.map { NSColor(srgbRed: Double($0[0]) / 255, green: Double($0[1]) / 255, blue: Double($0[2]) / 255, alpha: 1) } ?? .labelColor
        return cell
    }
}

@MainActor
final class Rows: NSStackView {
    let body = NSStackView()
    let scroller = NSScroller()
    var changed: ((Int) -> Void)?
    var maximum = 0
    var start = 0
    var rowHeight: CGFloat = 32
    var fixedHeight: NSLayoutConstraint?

    init() {
        super.init(frame: .zero)
        orientation = .horizontal
        alignment = .top
        spacing = 2
        body.orientation = .vertical
        body.alignment = .leading
        body.spacing = 0
        addArrangedSubview(body)
        addArrangedSubview(scroller)
        scroller.target = self
        scroller.action = #selector(scrollChanged)
        body.translatesAutoresizingMaskIntoConstraints = false
        scroller.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            scroller.widthAnchor.constraint(equalToConstant: 16),
            scroller.heightAnchor.constraint(equalTo: heightAnchor),
            body.widthAnchor.constraint(equalTo: widthAnchor, constant: -18)
        ])
    }

    required init?(coder _: NSCoder) { fatalError("init(coder:) is unavailable") }

    @objc func scrollChanged() {
        let next: Int
        switch scroller.hitPart {
        case .incrementLine: next = start + 1
        case .decrementLine: next = start - 1
        case .incrementPage: next = start + 6
        case .decrementPage: next = start - 6
        default: next = Int((scroller.doubleValue * Double(maximum)).rounded())
        }
        changed?(max(0, min(maximum, next)))
    }

    override func scrollWheel(with event: NSEvent) {
        let delta = event.scrollingDeltaY
        if delta != 0 {
            changed?(max(0, min(maximum, start + (delta < 0 ? 1 : -1))))
        }
    }
}

@MainActor
final class DocumentStack: NSStackView {
    override var isFlipped: Bool { true }
}

/// Equal-width grid with explicit placement. Children are the usual wrapper
/// frames, positioned manually: the Rust side resolves default-cell flow via
/// the shared `grid_position` helper and forwards final row/column/spans
/// through `w3_place`, so Swift only honors what it is given. Row heights
/// come from natural (or explicit style) child heights, distributed across
/// spans like the Tao backend; columns split the available width evenly.
@MainActor
final class GridContainer: NSView {
    var columns = 2
    var gap: CGFloat = 8
    var placements: [String: (row: Int, col: Int, rowSpan: Int, colSpan: Int)] = [:]
    var explicitSizes: [String: NSSize] = [:]
    private var lastTotal = NSSize(width: 0, height: 0)
    override var isFlipped: Bool { true }

    static func childKey(_ view: NSView) -> String? {
        guard let raw = view.identifier?.rawValue, raw.hasPrefix("frame/") else { return nil }
        return String(raw.dropFirst(6))
    }
    func measured(_ view: NSView, key: String) -> NSSize {
        let fit = view.fittingSize
        let exp = explicitSizes[key] ?? NSSize(width: -1, height: -1)
        return NSSize(width: exp.width >= 0 ? exp.width : max(0, fit.width),
                      height: exp.height >= 0 ? exp.height : max(0, fit.height))
    }
    private func compute(width: CGFloat) -> (cells: [(view: NSView, rect: NSRect)], total: NSSize) {
        let cols = max(1, columns)
        let cellW: CGFloat = width > 0 ? (width - gap * CGFloat(cols - 1)) / CGFloat(cols) : 0
        var items: [(view: NSView, key: String, row: Int, col: Int, rowSpan: Int, colSpan: Int, size: NSSize)] = []
        for sub in subviews {
            guard let key = GridContainer.childKey(sub), !sub.isHidden else { continue }
            let p = placements[key] ?? (row: 0, col: 0, rowSpan: 1, colSpan: 1)
            let col = min(max(0, p.col), cols - 1)
            let row = max(0, p.row)
            let colSpan = min(max(1, p.colSpan), cols - col)
            let rowSpan = max(1, p.rowSpan)
            items.append((sub, key, row, col, rowSpan, colSpan, measured(sub, key: key)))
        }
        let rowCount = items.map { $0.row + $0.rowSpan }.max() ?? 0
        var heights = [CGFloat](repeating: 0, count: rowCount)
        for item in items where item.rowSpan == 1 {
            heights[item.row] = max(heights[item.row], item.size.height)
        }
        for item in items where item.rowSpan > 1 {
            let occupied = heights[item.row..<(item.row + item.rowSpan)].reduce(0, +)
                + gap * CGFloat(item.rowSpan - 1)
            if item.size.height > occupied {
                let extra = (item.size.height - occupied) / CGFloat(item.rowSpan)
                for r in item.row..<(item.row + item.rowSpan) { heights[r] += extra }
            }
        }
        var tops = [CGFloat](repeating: 0, count: rowCount + 1)
        for r in 0..<rowCount { tops[r + 1] = tops[r] + heights[r] + (r + 1 < rowCount ? gap : 0) }
        var cells: [(view: NSView, rect: NSRect)] = []
        for item in items {
            let w: CGFloat
            if width > 0 {
                w = cellW * CGFloat(item.colSpan) + gap * CGFloat(item.colSpan - 1)
            } else {
                w = item.size.width
            }
            let h = heights[item.row..<(item.row + item.rowSpan)].reduce(0, +)
                + gap * CGFloat(item.rowSpan - 1)
            let x = width > 0 ? CGFloat(item.col) * (cellW + gap) : 0
            cells.append((item.view, NSRect(x: x, y: tops[item.row], width: w, height: max(h, item.size.height))))
        }
        let total = NSSize(width: width, height: tops.last ?? 0)
        return (cells, total)
    }
    override var intrinsicContentSize: NSSize {
        NSSize(width: NSView.noIntrinsicMetric, height: lastTotal.height)
    }
    override func layout() {
        super.layout()
        let (cells, total) = compute(width: bounds.width)
        for cell in cells { cell.view.frame = cell.rect }
        if total != lastTotal {
            lastTotal = total
            invalidateIntrinsicContentSize()
        }
    }
}

/// Horizontal wrapping flow with a uniform gap. Child order is mount order
/// (declaration order); each child takes its natural or explicit style size.
@MainActor
final class FlexContainer: NSView {
    var gap: CGFloat = 8
    var explicitSizes: [String: NSSize] = [:]
    private var lastTotal = NSSize(width: 0, height: 0)
    override var isFlipped: Bool { true }

    private func compute(width: CGFloat) -> (cells: [(view: NSView, rect: NSRect)], total: NSSize) {
        var cells: [(view: NSView, rect: NSRect)] = []
        var x: CGFloat = 0
        var y: CGFloat = 0
        var rowH: CGFloat = 0
        for sub in subviews {
            guard let key = GridContainer.childKey(sub), !sub.isHidden else { continue }
            let fit = sub.fittingSize
            let exp = explicitSizes[key] ?? NSSize(width: -1, height: -1)
            let w = exp.width >= 0 ? exp.width : max(0, fit.width)
            let h = exp.height >= 0 ? exp.height : max(0, fit.height)
            if width > 0, x > 0, x + w > width {
                x = 0
                y += rowH + gap
                rowH = 0
            }
            cells.append((sub, NSRect(x: x, y: y, width: w, height: h)))
            x += w + gap
            rowH = max(rowH, h)
        }
        return (cells, NSSize(width: width, height: y + rowH))
    }
    override var intrinsicContentSize: NSSize {
        NSSize(width: NSView.noIntrinsicMetric, height: lastTotal.height)
    }
    override func layout() {
        super.layout()
        let (cells, total) = compute(width: bounds.width)
        for cell in cells { cell.view.frame = cell.rect }
        if total != lastTotal {
            lastTotal = total
            invalidateIntrinsicContentSize()
        }
    }
}

/// Fixed-origin container. Positions arrive via `w3_place` (x/y in logical
/// points, y growing downward like the flipped document stack); sizes are
/// natural or explicit style sizes.
@MainActor
final class AbsoluteContainer: NSView {
    var positions: [String: NSPoint] = [:]
    var explicitSizes: [String: NSSize] = [:]
    private var lastTotal = NSSize(width: 0, height: 0)
    override var isFlipped: Bool { true }

    private func compute() -> (cells: [(view: NSView, rect: NSRect)], total: NSSize) {
        var cells: [(view: NSView, rect: NSRect)] = []
        var extent = NSSize(width: 0, height: 0)
        for sub in subviews {
            guard let key = GridContainer.childKey(sub), !sub.isHidden else { continue }
            let fit = sub.fittingSize
            let exp = explicitSizes[key] ?? NSSize(width: -1, height: -1)
            let w = exp.width >= 0 ? exp.width : max(0, fit.width)
            let h = exp.height >= 0 ? exp.height : max(0, fit.height)
            let origin = positions[key] ?? NSPoint.zero
            cells.append((sub, NSRect(x: origin.x, y: origin.y, width: w, height: h)))
            extent.width = max(extent.width, origin.x + w)
            extent.height = max(extent.height, origin.y + h)
        }
        return (cells, extent)
    }
    override var intrinsicContentSize: NSSize {
        NSSize(width: max(0, lastTotal.width), height: lastTotal.height)
    }
    override func layout() {
        super.layout()
        let (cells, total) = compute()
        for cell in cells { cell.view.frame = cell.rect }
        if total != lastTotal {
            lastTotal = total
            invalidateIntrinsicContentSize()
        }
    }
}

@MainActor
final class Scrolling: NSScrollView {
    let body = DocumentStack()

    init(horizontal: Bool) {
        super.init(frame: .zero)
        hasHorizontalScroller = horizontal
        hasVerticalScroller = !horizontal
        scrollerStyle = .overlay
        autohidesScrollers = true
        drawsBackground = false
        body.orientation = .vertical
        body.alignment = .leading
        body.spacing = 0
        documentView = body
        body.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            body.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            body.topAnchor.constraint(equalTo: contentView.topAnchor),
            horizontal
                ? body.heightAnchor.constraint(equalTo: contentView.heightAnchor)
                : body.widthAnchor.constraint(equalTo: contentView.widthAnchor)
        ])
    }

    required init?(coder _: NSCoder) { fatalError("coder construction unsupported") }
}

@MainActor
final class ApplicationDelegate: NSObject, NSApplicationDelegate {
    static let shared = ApplicationDelegate()
    weak var active: Host?
    @objc func quitRequested() { active?.emit("", kind: 3) }
    func retire(_ value: Host) {
        guard active === value else { return }
        active = nil
        let next = NSApp.orderedWindows.compactMap { $0.delegate as? Host }.first {
            !$0.closed && ($0.window.isVisible || $0.window.isMiniaturized)
        }
        if let next = next {
            next.window.makeKeyAndOrderFront(nil)
            next.windowDidBecomeKey(Notification(name: NSWindow.didBecomeKeyNotification))
        } else {
            NSApp.mainMenu = nil
        }
    }
    func applicationShouldTerminate(_ _: NSApplication) -> NSApplication.TerminateReply {
        quitRequested()
        return .terminateCancel
    }
}

@MainActor
final class Tabs: NSStackView, NSTableViewDataSource, NSTableViewDelegate {
    let header = NSSegmentedControl()
    let body = NSTabView()
    let list = NSTableView()
    let rail = NSScrollView()
    var labels: [String] = []
    var enabled = true
    private var floorConstraint: NSLayoutConstraint?
    private var exactHeight: NSLayoutConstraint?
    private var topBodyHeight: NSLayoutConstraint?
    private lazy var bodyWidth = body.widthAnchor.constraint(equalTo: widthAnchor)
    private lazy var verticalBodyWidth = body.widthAnchor.constraint(equalTo: widthAnchor, constant: -128)
    private lazy var bodyHeight = body.heightAnchor.constraint(equalTo: heightAnchor)
    private lazy var railWidth = rail.widthAnchor.constraint(equalToConstant: 120)
    private lazy var railHeight = rail.heightAnchor.constraint(equalTo: heightAnchor)
    override init(frame: NSRect) {
        super.init(frame: frame)
        orientation = .vertical
        alignment = .leading
        spacing = 8
        header.trackingMode = .selectOne
        header.segmentStyle = .rounded
        body.tabViewType = .noTabsNoBorder
        distribution = .fill
        body.setContentHuggingPriority(.defaultLow, for: .vertical)
        header.setContentHuggingPriority(.defaultHigh, for: .vertical)
        addArrangedSubview(header)
        addArrangedSubview(body)
        body.translatesAutoresizingMaskIntoConstraints = false
        // HIG sidebar: a source-list table on sidebar material with overlay
        // scrollers, like Settings/Finder navigation. No icons exist in the
        // page model, so rows are labels only at the sidebar text size.
        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("sidebar"))
        column.resizingMask = .autoresizingMask
        list.addTableColumn(column)
        list.headerView = nil
        list.dataSource = self
        list.delegate = self
        list.backgroundColor = .clear
        list.selectionHighlightStyle = .sourceList
        list.allowsEmptySelection = false
        list.allowsColumnReordering = false
        list.allowsColumnResizing = false
        list.focusRingType = .none
        list.gridStyleMask = []
        list.rowHeight = 28
        list.columnAutoresizingStyle = .lastColumnOnlyAutoresizingStyle
        list.translatesAutoresizingMaskIntoConstraints = false
        let sidebar = NSVisualEffectView()
        sidebar.material = .sidebar
        sidebar.blendingMode = .behindWindow
        sidebar.state = .active
        sidebar.translatesAutoresizingMaskIntoConstraints = false
        rail.addSubview(sidebar, positioned: .below, relativeTo: rail.contentView)
        NSLayoutConstraint.activate([
            sidebar.leadingAnchor.constraint(equalTo: rail.leadingAnchor),
            sidebar.trailingAnchor.constraint(equalTo: rail.trailingAnchor),
            sidebar.topAnchor.constraint(equalTo: rail.topAnchor),
            sidebar.bottomAnchor.constraint(equalTo: rail.bottomAnchor),
        ])
        rail.drawsBackground = false
        rail.contentView.drawsBackground = false
        rail.scrollerStyle = .overlay
        rail.autohidesScrollers = true
        rail.hasVerticalScroller = true
        rail.translatesAutoresizingMaskIntoConstraints = false
        rail.documentView = list
        list.widthAnchor.constraint(equalTo: rail.contentView.widthAnchor).isActive = true
        // NOTE: no height tie between the list and the clip. A document
        // height constraint against the enclosing clip (even advisory) made
        // AppKit shrink the whole window via
        // _changeWindowFrameFromConstraintsIfNecessary. The rail carries
        // full-height sidebar material, so the content-sized table reads as
        // a top-aligned source list like Finder/Settings.
        // NSTabView reports no useful fitting height, so Top tab bodies
        // would collapse to zero: pin the body to the selected page height
        // instead (vertical tabs bind body to the Tabs height).
        topBodyHeight = body.heightAnchor.constraint(greaterThanOrEqualToConstant: 0)
        setPosition("Top")
    }
    func setPosition(_ position: String) {
        let vertical = position == "Left" || position == "Right"
        bodyWidth.isActive = false
        verticalBodyWidth.isActive = false
        bodyHeight.isActive = false
        topBodyHeight?.isActive = false
        railWidth.isActive = false
        railHeight.isActive = false
        for view in arrangedSubviews { removeArrangedSubview(view); view.removeFromSuperview() }
        orientation = vertical ? .horizontal : .vertical
        alignment = vertical ? .top : .leading
        let headings: NSView = vertical ? rail : header
        if position == "Bottom" || position == "Right" {
            addArrangedSubview(body); addArrangedSubview(headings)
        } else { addArrangedSubview(headings); addArrangedSubview(body) }
        bodyWidth.isActive = !vertical
        verticalBodyWidth.isActive = vertical
        bodyHeight.isActive = vertical
        topBodyHeight?.isActive = !vertical
        railWidth.isActive = vertical
        railHeight.isActive = vertical
        syncTopBodyHeight()
    }
    /// NSTabView has no intrinsic height: without this, Top tab bodies
    /// collapse to zero and their pages never paint. Tracks the selected
    /// page's fitting height as a floor (change-guarded, so layout
    /// converges): an exact height would steal the stack surplus that
    /// belongs to the body, stretching the header instead and leaving
    /// blank bands above and below the segments.
    func syncTopBodyHeight() {
        guard let height = topBodyHeight, height.isActive,
              let page = body.selectedTabViewItem?.view else { return }
        let fitting = page.fittingSize.height
        if fitting.isFinite, fitting >= 0, height.constant != fitting {
            height.constant = fitting
        }
    }
    override func layout() {
        super.layout()
        syncTopBodyHeight()
    }

    /// Size the Tabs to its content: tall content grows the document and
    /// scrolls (with a floor keeping the Tabs at least viewport-tall so
    /// the footer siblings never overlap it), while short content takes
    /// its natural height like the other backends. Deliberately no
    /// stretch-to-fill for Top/Bottom tabs: forcing short tab bodies to
    /// the viewport height leaves a blank band between the body and the
    /// footer siblings. Vertical (Left/Right) tabs always fill: the rail
    /// is a Finder-style sidebar that must stretch to the window height,
    /// and the body fills alongside it (content stays top-aligned, so no
    /// inter-view blank band); without this the Tabs collapses to content
    /// height, the rail clips to a few rows, and the footer rides up.
    /// Returns whether anything changed.
    @discardableResult
    func updateFloor() -> Bool {
        guard let clip = enclosingScrollView?.contentView, clip.bounds.height > 0 else { return false }
        // The Tabs view sits inside a wrapper frame; the footer siblings
        // (status line, OK/Cancel row) follow that frame in the root stack.
        // Prefer laid-out frame heights (exact); fall back to fitting sizes
        // before first layout. A safety margin keeps the footer fully
        // visible against margin accounting drift (clipped buttons are far
        // worse than a few points of dead space).
        var footer: CGFloat = 40
        if let frame = superview as? NSStackView,
           let root = frame.superview as? NSStackView,
           let index = root.arrangedSubviews.firstIndex(of: frame)
        {
            for sibling in root.arrangedSubviews[(index + 1)...] {
                footer += max(sibling.frame.height, max(0, sibling.fittingSize.height)) + root.spacing
            }
        }
        let fill = max(0, clip.bounds.height - footer)
        // Content truth is the selected page's fitting: the NSTabView body
        // reports no intrinsic height, and our own height constraints would
        // pollute a whole-Tabs fitting read.
        var content = body.selectedTabViewItem?.view?.fittingSize.height ?? -1
        if content >= 0, orientation == .vertical {
            content = max(content, list.fittingSize.height)
        } else if content >= 0 {
            content += header.fittingSize.height
        }
        guard content >= 0 else { return false }
        // Vertical (Left/Right) tabs always fill the viewport: the rail is
        // a sidebar that must stretch to the window height. Top/Bottom tabs
        // keep the short-content natural height to avoid a blank band.
        let verticalTabs = orientation == .horizontal
        if content >= fill || verticalTabs {
            // Tall content (or any vertical-tab content): floor only; the
            // exact height must stay off or it fights the natural size.
            if let exact = exactHeight, exact.isActive {
                exact.isActive = false
            }
            if let floor = floorConstraint {
                if floor.constant != -footer {
                    floor.constant = -footer
                    floor.isActive = true
                    return true
                }
                if !floor.isActive {
                    floor.isActive = true
                    return true
                }
            } else {
                let floor = heightAnchor.constraint(greaterThanOrEqualTo: clip.heightAnchor)
                floor.constant = -footer
                floor.isActive = true
                floorConstraint = floor
                return true
            }
        } else {
            // Short Top/Bottom content: natural height (no stretch).
            // Vertical tabs never reach here (always fill above).
            if let exact = exactHeight, exact.isActive {
                exact.isActive = false
                return true
            }
            if let floor = floorConstraint, floor.isActive {
                floor.isActive = false
                return true
            }
        }
        return false
    }
    func rebuildHeader() {
        header.segmentCount = body.numberOfTabViewItems
        for (index, item) in body.tabViewItems.enumerated() {
            header.setLabel(item.label, forSegment: index)
        }
        labels = body.tabViewItems.map { $0.label }
        list.reloadData()
        // Pages can mount after the selection binding fired (dynamic
        // TabItems with an initially empty selection): with nothing
        // selected the tab view stays blank, so fall back to the first
        // page like the other backends do.
        if body.selectedTabViewItem == nil, body.numberOfTabViewItems > 0 {
            body.selectTabViewItem(at: 0)
        }
        refreshSelection()
    }
    func activatePage(_ index: Int) {
        guard index >= 0, index < body.numberOfTabViewItems else { return }
        body.selectTabViewItem(at: index)
        refreshSelection()
    }
    func refreshSelection() {
        let selected = body.selectedTabViewItem.map { body.indexOfTabViewItem($0) } ?? -1
        header.selectedSegment = selected
        if selected >= 0, selected < labels.count {
            if list.selectedRow != selected {
                list.selectRowIndexes(IndexSet(integer: selected), byExtendingSelection: false)
            }
            list.scrollRowToVisible(selected)
        } else {
            list.deselectAll(nil)
        }
        syncTopBodyHeight()
    }
    func setEnabled(_ enabled: Bool) {
        // Change-guarded: reloadData retiles the rail and its verification
        // posts a (spurious) selection change, whose observer runs a full
        // fitting-size solve. During bulk builds every button bind refreshes
        // enablement, so an unconditional reload turns opens into O(n^2).
        guard self.enabled != enabled else { return }
        self.enabled = enabled
        header.isEnabled = enabled
        list.reloadData()
    }
    func numberOfRows(in _: NSTableView) -> Int { labels.count }
    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        guard let id = tableColumn?.identifier else { return nil }
        // Rows are 28pt while the label is ~16pt: center the text
        // vertically in a reused container. Returning the label itself
        // leaves it at its fitting height pinned to the row top.
        let cell: NSTableCellView
        let field: NSTextField
        if let reused = tableView.makeView(withIdentifier: id, owner: self) as? NSTableCellView,
           let reusedField = reused.textField {
            cell = reused
            field = reusedField
        } else {
            cell = NSTableCellView()
            cell.identifier = id
            field = NSTextField(labelWithString: "")
            field.translatesAutoresizingMaskIntoConstraints = false
            cell.addSubview(field)
            NSLayoutConstraint.activate([
                field.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 2),
                field.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -2),
                field.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            ])
            cell.textField = field
        }
        field.identifier = id
        field.stringValue = labels[row]
        field.font = .systemFont(ofSize: 13)
        return cell
    }
    func tableView(_ _: NSTableView, shouldSelectRow _: Int) -> Bool { enabled }
    func tableViewSelectionDidChange(_ _: Notification) {
        let row = list.selectedRow
        guard enabled, row >= 0, row < body.numberOfTabViewItems else { return }
        // Spurious verification posts (retile, reloadData) re-announce the
        // current row: selecting again is cheap, but the refresh below runs
        // a full fitting-size solve, so skip it when nothing changed.
        let current = body.selectedTabViewItem.map { body.indexOfTabViewItem($0) } ?? -1
        guard current != row else { return }
        body.selectTabViewItem(at: row)
        // Reconcile when the tab refuses (readonly): the body still holds
        // the previous page, so reselect its row.
        refreshSelection()
    }
    required init?(coder _: NSCoder) { fatalError("coder construction unsupported") }
}

@MainActor
final class PointerImage: NSImageView {
    weak var owner: Host?
    var key = ""
    var pointerEnabled = false
    /// Fill mode (game surface): the view fills the window like the
    /// tao/gtk surfaces instead of sizing from source pixels. Natural-size
    /// constraints never apply; aspect is kept by imageScaling.
    var fill = false
    private var pointerArea: NSTrackingArea?
    /// Source pixel size (0 when no image); drives natural sizing per the
    /// shared layout priority (explicit placement wins, natural source
    /// size otherwise — never aspect-derived growth).
    var sourceWidth: CGFloat = 0
    var sourceHeight: CGFloat = 0
    /// Last explicit style size per axis (-1 = natural). Stored so image
    /// data arriving after styling can fill in the natural axes.
    var styleWidth: Double = -1
    var styleHeight: Double = -1
    var styleHorizontal: Int32 = 0
    var styleLeft: Double = 0
    var styleRight: Double = 0
    /// Frame constraints derived from the source size (tracked so data
    /// updates can replace them without touching style constraints).
    var naturalDims: [NSLayoutConstraint] = []
    private var lastEmittedSize = NSZeroSize
    override var isFlipped: Bool { true }
    // Frame dimensions (explicit placement or source size, managed via
    // dimensions[]) drive layout; keep intrinsic out of it so pixels
    // never size the window on their own.
    override var intrinsicContentSize: NSSize { .zero }

    func updateSourceSize(_ width: Int, _ height: Int) {
        sourceWidth = width > 0 && height > 0 ? CGFloat(width) : 0
        sourceHeight = width > 0 && height > 0 ? CGFloat(height) : 0
        invalidateIntrinsicContentSize()
    }

    override func layout() {
        super.layout()
        // Report the live window content size (not the view bounds): it is
        // the viewport truth the renderer must match, like tao's window
        // inner size. Change-gated so steady state stays silent.
        guard let content = owner?.window.contentView?.bounds.size,
              content.width > 0, content.height > 0, content != lastEmittedSize else { return }
        lastEmittedSize = content
        if let owner, !owner.closed {
            owner.emit("", "\(content.width),\(content.height)", kind: 8)
        }
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let area = pointerArea { removeTrackingArea(area) }
        let area = NSTrackingArea(rect: bounds, options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self)
        addTrackingArea(area)
        pointerArea = area
    }

    override func mouseMoved(with event: NSEvent) {
        guard pointerEnabled, let owner, !owner.closed, owner.interactive(key) else { return }
        let point = convert(event.locationInWindow, from: nil)
        guard let data = try? JSONSerialization.data(withJSONObject: [point.x, point.y]),
              let value = String(data: data, encoding: .utf8) else { return }
        owner.emit(key, value)
    }

    override func mouseExited(with _: NSEvent) {
        guard pointerEnabled, let owner, !owner.closed, owner.interactive(key) else { return }
        owner.emit(key, "null")
    }
}

@MainActor
final class Host: NSObject, NSWindowDelegate, NSTextFieldDelegate, NSTabViewDelegate {
    let window: NSWindow
    let root = DocumentStack()
    var nodes: [String: NSView] = [:]
    var parents: [String: String] = [:]
    var floorRecheckPending = false
    var lastFloorUpdate = 0.0
    var visibility: [String: Bool] = [:]
    var hiddenIndex: [String: Int] = [:]
    var enablement: [String: Bool] = [:]
    var capabilities: [String: Bool] = [:]
    var frames: [String: NSStackView] = [:]
    var dimensions: [String: [NSLayoutConstraint]] = [:]
    var spacers: [String: NSView] = [:]
    var contentWidths: [String: NSLayoutConstraint] = [:]
    var contentHeights: [String: NSLayoutConstraint] = [:]
    var menus: [String: NSMenu] = [:]
    var menuItems: [String: NSMenuItem] = [:]
    /// Whether the view installed a real application menu (replacing the
    /// Host stub). Hosts without one never touch the shared menubar, so a
    /// secondary window keeps showing the main window's menus.
    var ownsAppMenu = false
    var tabItems: [String: NSTabViewItem] = [:]
    var dialogs: [String: NSPanel] = [:]
    var readonlyTabs: Set<String> = []
    var readonlyDialogs: Set<String> = []
    var booleanButtons: Set<String> = []
    var applying = false
    var focusable: [NSView] = []
    var callback: Callback?
    var context: UnsafeMutableRawPointer?
    var closed = false
    weak var editingProbe: NSTextView?
    var editingProbeText = ""
    var inputMonitor: Any?
    var pressedCodes = Set<UInt16>()
    let bar = NSMenu()

    override init() {
        precondition(Thread.isMainThread)
        let initial = pendingInitialSize ?? NSSize(width: 760, height: 660)
        pendingInitialSize = nil
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: initial.width, height: initial.height),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable],
                          backing: .buffered, defer: false)
        super.init()
        window.isReleasedWhenClosed = false
        window.delegate = self
        window.autorecalculatesKeyViewLoop = false
        window.center()
        // Game content owns every pixel: transient gaps (live resize before
        // the next capture lands) must read as black like the tao/gtk
        // surfaces, never the OS window gray.
        window.backgroundColor = .black
        // Live resize must show live frames, not a stretched cache: the
        // render loop keeps producing frames through the drag.
        window.preservesContentDuringLiveResize = false
        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.drawsBackground = false
        scroll.contentView.drawsBackground = false
        // Overlay scrollers never reserve layout space (a legacy scroller
        // appearing over an exact-fit game view would shrink the clip and
        // feed a layout loop), and exact-fit content must not bounce: the
        // outer document never scrolls spuriously while inner scrollers
        // (tables, tab rails) keep their own behavior.
        scroll.scrollerStyle = .overlay
        scroll.verticalScrollElasticity = .none
        scroll.horizontalScrollElasticity = .none
        window.contentView = scroll
        root.orientation = .vertical
        root.alignment = .leading
        root.spacing = 0
        scroll.documentView = root
        root.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            root.leadingAnchor.constraint(equalTo: scroll.contentView.leadingAnchor),
            root.topAnchor.constraint(equalTo: scroll.contentView.topAnchor),
            root.widthAnchor.constraint(equalTo: scroll.contentView.widthAnchor)
        ])
        nodes["__root"] = root
        frames["__root"] = root
        NSApp.delegate = ApplicationDelegate.shared
        let app = NSMenuItem()
        let appMenu = NSMenu(title: "Panechord")
        app.submenu = appMenu
        bar.addItem(app)
        let about = NSMenuItem(title: "About Panechord", action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)), keyEquivalent: "")
        appMenu.addItem(about)
        appMenu.addItem(.separator())
        let quit = NSMenuItem(title: "Quit Panechord", action: #selector(ApplicationDelegate.quitRequested), keyEquivalent: "q")
        quit.target = ApplicationDelegate.shared
        appMenu.addItem(quit)
        inputMonitor = NSEvent.addLocalMonitorForEvents(matching: [.keyDown, .keyUp, .flagsChanged]) { [weak self] event in
            guard let self, !self.closed, event.window === self.window else { return event }
            let pressed: Bool
            if event.type == .flagsChanged {
                pressed = !self.pressedCodes.contains(event.keyCode)
            } else {
                pressed = event.type == .keyDown
            }
            if pressed { self.pressedCodes.insert(event.keyCode) }
            else { self.pressedCodes.remove(event.keyCode) }
            let repeated = event.type == .keyDown && event.isARepeat
            let kind: Int
            if pressed { kind = repeated ? 7 : 5 } else { kind = 6 }
            self.emit("", String(event.keyCode), kind: kind)
            return event
        }
    }

    func stopInput() {
        if let inputMonitor { NSEvent.removeMonitor(inputMonitor) }
        inputMonitor = nil
        pressedCodes.removeAll()
    }

    func windowDidBecomeKey(_ _: Notification) {
        guard !closed else { return }
        ApplicationDelegate.shared.active = self
        // Windows that installed any menu own the shared menubar while key.
        // Hosts without menus (dialogs, secondary windows) never touch it,
        // so they keep showing the main window's menus. The "app" menu is
        // just the HIG stub replacement; a main window without one (File,
        // States, ...) still needs its bar installed.
        if ownsAppMenu || !menus.isEmpty { NSApp.mainMenu = bar }
        // Reconcile tab floors now that the window is displayed and sibling
        // fitting sizes are valid (in-flight layout passes can deadlock on
        // pre-layout zeros, pushing footers below the fold).
        updateFloors()
        emit("", "1", kind: 4)
    }

    /// Refresh every Tabs floor constant from current sibling sizes.
    /// Change-guarded, so settled windows are untouched.
    func updateFloors() {
        var changed = false
        for view in nodes.values {
            guard let tabs = view as? Tabs else { continue }
            changed = tabs.updateFloor() || changed
        }
        for (id, view) in nodes {
            guard let table = view as? TableControl else { continue }
            changed = updateTableFloor(id: id, table: table) || changed
        }
        // Measurements during build/focus can precede layout (zeros); one
        // deferred pass runs after layout with valid sizes. One-shot per
        // change batch: settled state schedules nothing further.
        if changed, !floorRecheckPending {
            floorRecheckPending = true
            DispatchQueue.main.async { [weak self] in
                guard let self, !self.closed else { return }
                self.floorRecheckPending = false
                self.updateFloors()
            }
        }
    }

    /// Floor viewport-filling tables (no explicit height) to the visible clip
    /// minus their stack siblings: without it gravity sizes the scroll view
    /// by fitting (zero), collapsing the grid and letting following siblings
    /// ride up over the header. Mirrors Tabs.updateFloor.
    @discardableResult
    func updateTableFloor(id: String, table: TableControl) -> Bool {
        // Measure after layout: pre-layout frames are zero and scroll-view
        // fittings under-read, which would oversize the floor and push the
        // footer out of the window.
        window.layoutIfNeeded()
        guard table.fillsViewport,
              let frame = frames[id],
              let parentId = parents[id],
              let parent = nodes[parentId] as? NSStackView,
              parent.orientation == .vertical,
              let clip = frame.enclosingScrollView?.contentView ?? (window.contentView as? NSScrollView)?.contentView,
              clip.bounds.height > 0 else {
            if let floor = table.floorConstraint, floor.isActive {
                floor.isActive = false
                return true
            }
            return false
        }
        var siblings: CGFloat = 0
        for sub in parent.arrangedSubviews where sub !== frame {
            let h = max(sub.frame.height, max(0, sub.fittingSize.height))
            siblings += h + parent.spacing
        }
        // Padding lives on the wrapper frame (child insets), not the stack.
        let insets = frames[parentId]?.edgeInsets ?? NSEdgeInsetsZero
        siblings += insets.top + insets.bottom + 8
        let want = max(0, clip.bounds.height - siblings)
        if let floor = table.floorConstraint {
            if floor.constant != want {
                floor.constant = want
                if !floor.isActive { floor.isActive = true }
                return true
            }
            if !floor.isActive {
                floor.isActive = true
                return true
            }
        } else {
            let floor = frame.heightAnchor.constraint(greaterThanOrEqualToConstant: want)
            floor.isActive = true
            table.floorConstraint = floor
            return true
        }
        return false
    }

    func windowDidResignKey(_ _: Notification) {
        pressedCodes.removeAll()
        emit("", "0", kind: 4)
    }

    func windowDidResize(_ _: Notification) {
        guard !closed else { return }
        // The clip changed size: re-resolve tab floors to the new viewport.
        updateFloors()
        // Force synchronous layout now: during a live drag AppKit may defer
        // subview layout until mouse-up, leaving the game frame (and the
        // viewport size it reports) a step behind the window.
        window.layoutIfNeeded()
        emitSurfaceSize()
        // Live resize runs a nested tracking loop inside `sendEvent`, so the
        // host pump never returns until mouse-up and frame tasks stall.
        // This delegate fires inside that loop, so pump one executor turn
        // here: ready (timer-driven) tasks capture at the new size mid-drag
        // instead of showing a stale frame until release.
        if window.inLiveResize {
            emit("", kind: 9)
        }
    }

    func emitSurfaceSize() {
        let size = window.contentView?.bounds.size ?? .zero
        emit("", "\(size.width),\(size.height)", kind: 8)
    }

    func emit(_ id: String, _ value: String = "", kind: Int32 = 0) {
        guard let callback = callback else { return }
        Array(id.utf8).withUnsafeBufferPointer { key in
            Array(value.utf8).withUnsafeBufferPointer { text in
                callback(context, kind, key.baseAddress, key.count, text.baseAddress, text.count)
            }
        }
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        if let dialog = dialogs.first(where: { $0.value === sender }) {
            if readonlyDialogs.contains(dialog.key) { return false }
            emit(dialog.key, "0")
            return false
        }
        emit("", kind: 1)
        return false
    }
    @objc func activated(_ sender: NSControl) {
        guard !applying, let id = sender.identifier?.rawValue else { return }
        if let slider = sender as? NSSlider { emit(id, String(slider.doubleValue)) }
        else if let header = sender as? NSSegmentedControl, let tabs = nodes[id] as? Tabs,
            header.selectedSegment >= 0, header.selectedSegment < tabs.body.numberOfTabViewItems {
            tabs.body.selectTabViewItem(at: header.selectedSegment)
        }
        else if let combo = sender as? NSPopUpButton { emit(id, combo.selectedItem?.representedObject as? String ?? "") }
        else if let button = sender as? NSButton, booleanButtons.contains(id) {
            emit(id, button.state == .on ? "1" : "0")
        } else { emit(id) }
    }
    func tabView(_ _: NSTabView, shouldSelect tabViewItem: NSTabViewItem?) -> Bool {
        applying || !readonlyTabs.contains(tabView.identifier?.rawValue ?? "")
    }
    /// Collapse hidden views: NSStackView keeps hidden arranged subviews in
    /// layout, leaving blank gaps (e.g. an unused input section). Detach
    /// from stack containers and reinsert at the recorded index on show.
    /// Tab pages keep plain isHidden (the tab view sizes to its selection).
    func setVisible(_ id: String, _ visible: Bool) {
        guard let frame = frames[id] else { return }
        if tabItems[id] != nil || dialogs[id] != nil {
            frame.isHidden = !visible
            return
        }
        // Manually laid out containers skip hidden children in `layout()`,
        // so a plain hidden flag collapses them without detaching.
        if let parent = parents[id], isManualContainer(parent) {
            frame.isHidden = !visible
            (nodes[parent] as? NSView)?.needsLayout = true
            return
        }
        if visible {
            if frame.superview == nil,
               let parent = parents[id],
               let container = stackContainer(forParent: parent)
            {
                let index = min(hiddenIndex.removeValue(forKey: id) ?? container.arrangedSubviews.count,
                                container.arrangedSubviews.count)
                container.insertArrangedSubview(frame, at: index)
            }
            frame.isHidden = false
        } else {
            guard let stack = frame.superview as? NSStackView,
                  let index = stack.arrangedSubviews.firstIndex(of: frame) else {
                frame.isHidden = true
                return
            }
            hiddenIndex[id] = index
            stack.removeArrangedSubview(frame)
            frame.removeFromSuperview()
        }
    }
    func stackContainer(forParent parent: String) -> NSStackView? {
        (nodes[parent] as? Rows)?.body
            ?? (nodes[parent] as? Scrolling)?.body
            ?? nodes[parent] as? NSStackView
    }
    func tabView(_ tabView: NSTabView, didSelect tabViewItem: NSTabViewItem?) {
        if let id = tabView.identifier?.rawValue, let tabs = nodes[id] as? Tabs, tabViewItem != nil {
            tabs.refreshSelection()
        }
        if !applying, let id = tabView.identifier?.rawValue, let key = tabViewItem?.identifier as? String { emit(id, key) }
    }
    @objc func menuActivated(_ sender: NSMenuItem) {
        if let id = sender.representedObject as? String { emit(id) }
    }
    func controlTextDidChange(_ notification: Notification) {
        guard let field = notification.object as? NSTextField,
              let id = field.identifier?.rawValue else { return }
        if let editor = field.currentEditor() as? NSTextView, editor.hasMarkedText() { return }
        emit(id, field.stringValue)
    }

    /// Whether the named node hosts manually positioned children
    /// (Grid/Flex/Absolute) rather than stack-arranged ones.
    func isManualContainer(_ id: String) -> Bool {
        nodes[id] is GridContainer || nodes[id] is FlexContainer || nodes[id] is AbsoluteContainer
    }

    /// Record an explicit size for a child of a manual container and
    /// schedule its layout. Sizes use -1 for the natural dimension.
    func recordManualSize(_ id: String, parent: String, width: Double, height: Double) {
        let size = NSSize(width: width, height: height)
        if let grid = nodes[parent] as? GridContainer {
            grid.explicitSizes[id] = size
            grid.needsLayout = true
        } else if let flex = nodes[parent] as? FlexContainer {
            flex.explicitSizes[id] = size
            flex.needsLayout = true
        } else if let absolute = nodes[parent] as? AbsoluteContainer {
            absolute.explicitSizes[id] = size
            absolute.needsLayout = true
        }
    }

    /// Reorder a container's children to the requested key order: stack
    /// containers re-append arranged subviews, manual containers re-append
    /// plain subviews so `Repeat`/`ContentControl` order matches everywhere.
    func reorderChildren(of id: String, to keys: [String]) {
        if let stack = nodes[id] as? NSStackView {
            for key in keys {
                guard let frame = frames[key], stack.arrangedSubviews.contains(frame) else { continue }
                stack.removeArrangedSubview(frame)
                stack.addArrangedSubview(frame)
            }
        } else if isManualContainer(id), let container = nodes[id] as? NSView {
            for key in keys {
                guard let frame = frames[key], frame.superview === container else { continue }
                frame.removeFromSuperview()
                container.addSubview(frame)
            }
            container.needsLayout = true
        }
    }

    /// A wrapper frame around a mounted control: vertical, leading-aligned,
    /// no spacing. Position constraints are the caller's job.
    func makeFrame(for view: NSView, id: String) -> NSStackView {
        let frame = NSStackView()
        frame.orientation = .vertical
        frame.alignment = .leading
        frame.spacing = 0
        frame.identifier = NSUserInterfaceItemIdentifier("frame/" + id)
        frame.addArrangedSubview(view)
        return frame
    }

    /// Link a control to its wrapper frame: the control fills the frame
    /// width (minus insets applied later), with height linked only for
    /// expanding kinds. Shared by stack and manual mounts.
    func linkContent(_ view: NSView, to frame: NSStackView, id: String) {
        view.translatesAutoresizingMaskIntoConstraints = false
        let contentWidth = view.widthAnchor.constraint(equalTo: frame.widthAnchor, constant: 0)
        contentWidth.isActive = true
        let contentHeight = view.heightAnchor.constraint(equalTo: frame.heightAnchor)
        contentHeight.isActive = view is Tabs || view is Scrolling || view is TableControl
            || (view as? PointerImage)?.fill == true
        contentWidths[id] = contentWidth
        contentHeights[id] = contentHeight
        // Images center within their frame like the other backends.
        if view is PointerImage { frame.alignment = .centerX }
    }

    /// Record a mounted control and wire it into the key-view loop.
    func registerNode(_ view: NSView, frame: NSStackView, parent: String, id: String) {
        nodes[id] = view
        parents[id] = parent
        frames[id] = frame
        view.identifier = NSUserInterfaceItemIdentifier(id)
        view.setAccessibilityIdentifier(id)
        if view is NSButton || (view as? NSTextField)?.isSelectable == true {
            focusable.append(view)
            keyLoop()
        }
    }

    /// Whether an explicit width belongs on the content node rather than
    /// its frame: true under vertical stacks (and unknown parents, which
    /// default vertical). Horizontal parents keep the frame-width path
    /// (verified by the rows meter). An exact frame width under a
    /// vertical stack lets AppKit derive the window width from content.
    func directVerticalParent(of id: String) -> Bool {
        guard let parent = parents[id] else { return true }
        if isManualContainer(parent) { return false }
        if let stack = nodes[parent] as? NSStackView { return stack.orientation == .vertical }
        return true
    }

    /// Fold margin/padding into the frame insets and update the content
    /// size constants. Shared by stack and manual style paths.
    func applyFrameInsets(_ id: String, top: Double, right: Double, bottom: Double, left: Double) {
        frames[id]?.edgeInsets = NSEdgeInsets(top: top, left: left, bottom: bottom, right: right)
        contentWidths[id]?.constant = -(left + right)
        contentHeights[id]?.constant = -(top + bottom)
    }

    func attach(_ view: NSView, parent: String, id: String, floating: Bool = false) {
        if !floating, isManualContainer(parent) {
            attachManualChild(view, parent: parent, id: id)
            return
        }
        let container = (nodes[parent] as? Rows)?.body ?? (nodes[parent] as? Scrolling)?.body ?? nodes[parent] as? NSStackView
        let tab = (nodes[parent] as? Tabs)?.body
        guard floating || container != nil || tab != nil else { return }
        let frame = makeFrame(for: view, id: id)
        if floating {
            let panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 480, height: 320),
                styleMask: [.titled, .closable], backing: .buffered, defer: false)
            panel.isReleasedWhenClosed = false
            panel.delegate = self
            panel.contentView = frame
            dialogs[id] = panel
        } else if let tab = tab {
            let item = NSTabViewItem(identifier: id)
            item.view = frame
            item.label = id
            applying = true
            tab.addTabViewItem(item)
            applying = false
            tabItems[id] = item
            if let tabs = nodes[parent] as? Tabs {
                tabs.rebuildHeader()
            }
        } else { container?.addArrangedSubview(frame) }
        frame.translatesAutoresizingMaskIntoConstraints = false
        if floating || tab != nil {
            frame.translatesAutoresizingMaskIntoConstraints = true
            frame.autoresizingMask = [.width, .height]
        }
        linkContent(view, to: frame, id: id)
        if let container = container, container.orientation == .vertical, !floating, tab == nil {
            frame.widthAnchor.constraint(equalTo: container.widthAnchor,
                constant: -(container.edgeInsets.left + container.edgeInsets.right)).isActive = true
        }
        // Game surfaces fill the visible area in both axes (no scrolling);
        // content images size from their pixels instead.
        if (view as? PointerImage)?.fill == true, !floating, tab == nil,
           let clip = (self.window.contentView as? NSScrollView)?.contentView {
            frame.heightAnchor.constraint(equalTo: clip.heightAnchor).isActive = true
        }
        registerNode(view, frame: frame, parent: parent, id: id)
    }

    /// Mount into a manually laid out container: the wrapper frame keeps its
    /// internal view/frame width link but takes no container width or position
    /// constraints — the container's `layout()` assigns frames from placements
    /// and explicit sizes instead.
    func attachManualChild(_ view: NSView, parent: String, id: String) {
        guard nodes[parent] != nil else { return }
        let frame = makeFrame(for: view, id: id)
        frame.translatesAutoresizingMaskIntoConstraints = true
        linkContent(view, to: frame, id: id)
        nodes[parent]?.addSubview(frame)
        registerNode(view, frame: frame, parent: parent, id: id)
        nodes[parent]?.needsLayout = true
    }

    func keyLoop() {
        for (index, view) in focusable.enumerated() {
            view.nextKeyView = focusable[(index + 1) % focusable.count]
        }
        window.initialFirstResponder = focusable.first
    }
    func interactive(_ id: String) -> Bool {
        var current: String? = id
        while let key = current {
            if visibility[key] == false || enablement[key] == false { return false }
            current = parents[key]
        }
        return true
    }
    func refreshEnablement() {
        for (id, view) in nodes {
            let enabled = interactive(id) && capabilities[id] != false
            // Change-guarded: touching isEnabled retiles table views and
            // their verification posts spurious selection changes, each of
            // which solves the whole constraint tree (see setEnabled).
            if let control = view as? NSControl, control.isEnabled != enabled {
                control.isEnabled = enabled
            }
            (view as? Tabs)?.setEnabled(enabled)
        }
    }

    func remove(_ prefix: String) {
        let previousApplying = applying
        applying = true
        defer { applying = previousApplying }
        let ids = nodes.keys.filter { $0 == prefix || $0.hasPrefix(prefix + "/") }
        for id in ids.sorted(by: { $0.count > $1.count }) {
            if let panel = dialogs.removeValue(forKey: id) {
                if panel.sheetParent != nil { window.endSheet(panel) }
                panel.orderOut(nil)
            }
            if let item = tabItems.removeValue(forKey: id) { item.tabView?.removeTabViewItem(item) }
            readonlyTabs.remove(id)
            readonlyDialogs.remove(id)
            booleanButtons.remove(id)
            if let frame = frames.removeValue(forKey: id) {
                if let parent = frame.superview as? NSStackView { parent.removeArrangedSubview(frame) }
                frame.removeFromSuperview()
            }
            dimensions.removeValue(forKey: id)
            contentWidths.removeValue(forKey: id)
            contentHeights.removeValue(forKey: id)
            parents.removeValue(forKey: id)
            visibility.removeValue(forKey: id)
            hiddenIndex.removeValue(forKey: id)
            for node in nodes.values {
                if let grid = node as? GridContainer {
                    grid.placements = grid.placements.filter { !($0.key == id || $0.key.hasPrefix(id + "/")) }
                    grid.explicitSizes = grid.explicitSizes.filter { !($0.key == id || $0.key.hasPrefix(id + "/")) }
                } else if let flex = node as? FlexContainer {
                    flex.explicitSizes = flex.explicitSizes.filter { !($0.key == id || $0.key.hasPrefix(id + "/")) }
                } else if let absolute = node as? AbsoluteContainer {
                    absolute.positions = absolute.positions.filter { !($0.key == id || $0.key.hasPrefix(id + "/")) }
                    absolute.explicitSizes = absolute.explicitSizes.filter { !($0.key == id || $0.key.hasPrefix(id + "/")) }
                }
            }
            if let spacer = spacers.removeValue(forKey: id) {
                if let stack = spacer.superview as? NSStackView {
                    stack.removeArrangedSubview(spacer)
                }
                spacer.removeFromSuperview()
            }
            enablement.removeValue(forKey: id)
            capabilities.removeValue(forKey: id)
            if let node = nodes.removeValue(forKey: id) { focusable.removeAll { $0 === node } }
        }
        keyLoop()
    }
}

@MainActor func host(_ pointer: UnsafeMutableRawPointer) -> Host {
    Unmanaged<Host>.fromOpaque(pointer).takeUnretainedValue()
}

@_cdecl("w3_create") @MainActor
public func create(_ callback: Callback?, _ context: UnsafeMutableRawPointer?) -> UnsafeMutableRawPointer {
    let app = NSApplication.shared
    app.setActivationPolicy(.regular)
    let value = Host()
    value.callback = callback
    value.context = context
    app.finishLaunching()
    return Unmanaged.passRetained(value).toOpaque()
}

@_cdecl("w3_destroy") @MainActor
public func destroy(_ pointer: UnsafeMutableRawPointer) {
    let value = host(pointer)
    value.callback = nil
    value.window.delegate = nil
    value.closed = true
    value.stopInput()
    ApplicationDelegate.shared.retire(value)
    value.window.close()
    Unmanaged<Host>.fromOpaque(pointer).release()
}

@_cdecl("w3_present") @MainActor
public func present(_ pointer: UnsafeMutableRawPointer) {
    host(pointer).window.makeKeyAndOrderFront(nil)
    host(pointer).windowDidBecomeKey(Notification(name: NSWindow.didBecomeKeyNotification))
    NSApp.activate(ignoringOtherApps: true)
}

@_cdecl("w3_close") @MainActor
public func close(_ pointer: UnsafeMutableRawPointer) {
    let value = host(pointer)
    value.closed = true
    value.stopInput()
    for panel in value.dialogs.values {
        if panel.sheetParent != nil { value.window.endSheet(panel) }
        panel.orderOut(nil)
    }
    value.window.orderOut(nil)
    ApplicationDelegate.shared.retire(value)
}

@_cdecl("w3_focused") @MainActor
public func focused(_ pointer: UnsafeMutableRawPointer) -> Int32 {
    NSApp.isActive && host(pointer).window.isKeyWindow ? 1 : 0
}

@_cdecl("w3_surface_view") @MainActor
public func surfaceView(_ pointer: UnsafeMutableRawPointer) -> UnsafeMutableRawPointer? {
    guard let view = host(pointer).window.contentView else { return nil }
    return Unmanaged.passUnretained(view).toOpaque()
}

@_cdecl("w3_surface_size") @MainActor
public func surfaceSize(_ pointer: UnsafeMutableRawPointer, _ width: UnsafeMutablePointer<Double>, _ height: UnsafeMutablePointer<Double>) {
    let size = host(pointer).window.contentView?.bounds.size ?? .zero
    width.pointee = size.width
    height.pointee = size.height
}

@_cdecl("w3_surface_scale") @MainActor
public func surfaceScale(_ pointer: UnsafeMutableRawPointer) -> Double {
    host(pointer).window.backingScaleFactor
}

@_cdecl("w3_toggle_fullscreen") @MainActor
public func toggleFullscreen(_ pointer: UnsafeMutableRawPointer) {
    host(pointer).window.toggleFullScreen(nil)
}

@_cdecl("w3_set_min_size") @MainActor
public func setMinSize(_ pointer: UnsafeMutableRawPointer, _ width: Double, _ height: Double) {
    // Logical points, not physical pixels: the image floor keeps source
    // pixels from squeezing, matching tao's LogicalSize minimum and the
    // spec's "window bounds win, content never resizes the window" floor.
    // (0,0) clears the minimum (no image yet).
    let value = host(pointer)
    if width <= 0 || height <= 0 {
        value.window.contentMinSize = .zero
    } else {
        value.window.contentMinSize = NSSize(width: width, height: height)
    }
}

@_cdecl("w3_set_automatic_window_tabbing") @MainActor
public func setAutomaticWindowTabbing(_ enabled: Int32) {
    NSWindow.allowsAutomaticWindowTabbing = enabled != 0
}

private var pendingInitialSize: NSSize?
@_cdecl("w3_set_initial_window_size") @MainActor
public func setInitialWindowSize(_ width: Double, _ height: Double) {
    guard width >= 100, height >= 100, width <= 8192, height <= 8192 else { return }
    pendingInitialSize = NSSize(width: width, height: height)
}

@_cdecl("w3_pick_file") @MainActor
public func pickFile(_ output: UnsafeMutablePointer<UInt8>?, _ capacity: Int) -> Int {
    let panel = NSOpenPanel()
    panel.canChooseFiles = true
    panel.canChooseDirectories = false
    panel.allowsMultipleSelection = false
    panel.resolvesAliases = true
    guard panel.runModal() == .OK, let url = panel.url else { return -1 }
    let bytes = Array(url.path.utf8)
    guard bytes.count <= capacity else { return -2 }
    if let output, capacity > 0 {
        output.update(from: bytes, count: bytes.count)
    }
    return bytes.count
}

@_cdecl("w3_render") @MainActor
public func render(_ pointer: UnsafeMutableRawPointer) {
    let value = host(pointer)
    value.window.contentView?.needsDisplay = true
    value.window.displayIfNeeded()
}

@_cdecl("w3_pump") @MainActor
public func pump(_ milliseconds: Int32) {
    autoreleasepool {
        // Live resize (and menu/modal tracking) runs the runloop in the
        // event-tracking mode, where a `.default`-only pump sees nothing and
        // the window appears frozen until mouse-up. While any window
        // live-resizes, drain tracking with the wait budget (default queues
        // nothing then) so frames keep flowing; otherwise poll normally.
        // until:nil returns immediately without waiting.
        let live = NSApp.windows.contains { $0.inLiveResize }
        if live {
            if let event = NSApp.nextEvent(matching: .any,
                until: nil, inMode: .default, dequeue: true) { NSApp.sendEvent(event) }
            else if let event = NSApp.nextEvent(matching: .any,
                until: Date(timeIntervalSinceNow: Double(milliseconds) / 1000),
                inMode: .eventTracking, dequeue: true) { NSApp.sendEvent(event) }
        } else if let event = NSApp.nextEvent(matching: .any,
            until: Date(timeIntervalSinceNow: Double(milliseconds) / 1000),
            inMode: .default, dequeue: true) { NSApp.sendEvent(event) }
        else if let event = NSApp.nextEvent(matching: .any,
            until: nil, inMode: .eventTracking, dequeue: true) { NSApp.sendEvent(event) }
        NSApp.updateWindows()
    }
}

@_cdecl("w3_add") @MainActor
public func add(_ pointer: UnsafeMutableRawPointer, _ parentBytes: UnsafePointer<UInt8>?, _ parentCount: Int,
         _ idBytes: UnsafePointer<UInt8>?, _ idCount: Int, _ kind: Int32, _ number: Double) {
    let value = host(pointer)
    let id = string(idBytes, idCount)
    let view: NSView
    switch kind {
    case 0, 1:
        let stack = NSStackView()
        stack.orientation = kind == 1 ? .horizontal : .vertical
        stack.alignment = kind == 1 ? .centerY : .leading
        stack.spacing = number
        view = stack
    case 2:
        let field = NSTextField(wrappingLabelWithString: "")
        field.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        field.setAccessibilityRole(.staticText)
        view = field
    case 3:
        let field = NSTextField(string: "")
        field.delegate = value
        field.isSelectable = true
        field.isEditable = number != 0
        field.setAccessibilityRole(.textField)
        field.usesSingleLineMode = true
        field.isAutomaticTextCompletionEnabled = false
        if let caption = value.nodes[id + "_caption"] as? NSTextField {
            field.setAccessibilityLabel(caption.stringValue)
        }
        field.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        view = field
    case 4:
        let button = NSButton(title: "", target: value, action: #selector(Host.activated(_:)))
        button.bezelStyle = .rounded
        view = button
    case 6, 7:
        value.booleanButtons.insert(id)
        let button = NSButton(title: "", target: value, action: #selector(Host.activated(_:)))
        button.setButtonType(kind == 6 ? .switch : .radio)
        view = button
    case 8:
        let slider = NSSlider(value: 0, minValue: 0, maxValue: 100, target: value, action: #selector(Host.activated(_:)))
        slider.isContinuous = true
        view = slider
    case 9:
        let combo = NSPopUpButton(frame: .zero, pullsDown: false)
        combo.target = value
        combo.action = #selector(Host.activated(_:))
        combo.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        view = combo
    case 10:
        let tabs = Tabs(frame: NSRect(x: 0, y: 0, width: 420, height: 180))
        tabs.body.delegate = value
        tabs.body.identifier = NSUserInterfaceItemIdentifier(id)
        tabs.header.identifier = NSUserInterfaceItemIdentifier(id)
        tabs.header.target = value
        tabs.header.action = #selector(Host.activated(_:))
        view = tabs
    case 11, 12:
        let stack = NSStackView()
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 8
        view = stack
    case 13, 23:
        let image = PointerImage()
        image.owner = value
        image.key = id
        image.imageScaling = .scaleProportionallyUpOrDown
        image.imageAlignment = .alignCenter
        image.setContentHuggingPriority(.defaultLow, for: .horizontal)
        image.setContentHuggingPriority(.defaultLow, for: .vertical)
        // Game surfaces (kind 23) fill the window; content images (kind 13)
        // keep natural source sizing.
        image.fill = kind == 23
        view = image
    case 15:
        view = Scrolling(horizontal: number != 0)
    case 16:
        let separator = NSBox()
        separator.boxType = .separator
        separator.setAccessibilityRole(.splitter)
        if number != 0 {
            separator.widthAnchor.constraint(equalToConstant: 1).isActive = true
        } else {
            separator.heightAnchor.constraint(equalToConstant: 1).isActive = true
        }
        view = separator
    case 17:
        let table = TableControl()
        table.owner = value
        table.key = id
        view = table
    case 18:
        let meter = NSProgressIndicator()
        meter.isIndeterminate = false
        meter.minValue = 0
        meter.maxValue = 1
        meter.style = .bar
        meter.setAccessibilityLabel("Progress")
        view = meter
    case 21:
        let flex = FlexContainer()
        flex.gap = CGFloat(number)
        view = flex
    case 22:
        view = AbsoluteContainer()
    default:
        let rows = Rows()
        rows.changed = { [weak value] start in value?.emit(id, String(start), kind: 2) }
        view = rows
    }
    value.attach(view, parent: string(parentBytes, parentCount), id: id, floating: kind == 12)
}

@_cdecl("w3_add_grid") @MainActor
public func addGrid(_ pointer: UnsafeMutableRawPointer, _ parentBytes: UnsafePointer<UInt8>?, _ parentCount: Int,
         _ idBytes: UnsafePointer<UInt8>?, _ idCount: Int, _ columns: UInt32, _ gap: Double) {
    let value = host(pointer)
    let id = string(idBytes, idCount)
    let grid = GridContainer()
    grid.columns = max(1, Int(columns))
    grid.gap = CGFloat(gap)
    value.attach(grid, parent: string(parentBytes, parentCount), id: id)
}

@_cdecl("w3_place") @MainActor
public func place(_ pointer: UnsafeMutableRawPointer, _ idBytes: UnsafePointer<UInt8>?, _ idCount: Int,
           _ row: Int32, _ column: Int32, _ rowSpan: Int32, _ columnSpan: Int32, _ x: Double, _ y: Double) {
    let value = host(pointer)
    let id = string(idBytes, idCount)
    guard let parent = value.parents[id] else { return }
    if let grid = value.nodes[parent] as? GridContainer {
        grid.placements[id] = (row: max(0, Int(row)), col: max(0, Int(column)),
            rowSpan: max(1, Int(rowSpan)), colSpan: max(1, Int(columnSpan)))
        grid.needsLayout = true
    } else if let absolute = value.nodes[parent] as? AbsoluteContainer {
        absolute.positions[id] = NSPoint(x: x, y: y)
        absolute.needsLayout = true
    }
}

@_cdecl("w3_set") @MainActor
public func set(_ pointer: UnsafeMutableRawPointer, _ idBytes: UnsafePointer<UInt8>?, _ idCount: Int,
         _ textBytes: UnsafePointer<UInt8>?, _ textCount: Int, _ property: Int32) {
    let value = host(pointer)
    let id = string(idBytes, idCount)
    let text = string(textBytes, textCount)
    if property == 20, let button = value.nodes[id] as? NSButton {
        // HIG default button: Return activates a non-destructive accept.
        // A destructive accept keeps no Return equivalent.
        button.keyEquivalent = text == "1" ? "\r" : ""
    } else if property == 21, let button = value.nodes[id] as? NSButton {
        // HIG cancel: Escape always activates cancel, destructive or not.
        button.keyEquivalent = text == "1" ? "\u{1b}" : ""
    } else if property == 19, let button = value.nodes[id] as? NSButton {
        // Destructive emphasis for dialog footers (macOS 11+, the minimum
        // deployment target): renders the accept button with the system
        // destructive style instead of leaving it unmarked.
        button.hasDestructiveAction = text == "1"
    } else if property == 18, let table = value.nodes[id] as? TableControl {
        table.selectionPresent = true
        table.selectionEnabled = text == "1"
        table.table.reloadData()
    } else if property == 17, let table = value.nodes[id] as? TableControl {
        table.setCursor(text)
    } else if property == 16,
       let data = text.data(using: .utf8), let keys = try? JSONDecoder().decode([String].self, from: data) {
        value.reorderChildren(of: id, to: keys)
    } else if property == 15, let image = value.nodes[id] as? PointerImage {
        image.pointerEnabled = text == "1"
    } else if property == 14, let field = value.nodes[id] as? NSTextField {
        field.placeholderString = text
    } else if property == 13, let table = value.nodes[id] as? TableControl {
        table.update(text)
    } else if property == 1 {
        value.capabilities[id] = text == "1"
        (value.nodes[id] as? NSControl)?.isEnabled = text == "1"
        if value.dialogs[id] != nil {
            if text == "1" { value.readonlyDialogs.remove(id) } else { value.readonlyDialogs.insert(id) }
        }
        if let tabs = value.nodes[id] as? Tabs {
            tabs.setEnabled(text == "1")
            if text == "1" { value.readonlyTabs.remove(id) } else { value.readonlyTabs.insert(id) }
        }
        value.refreshEnablement()
    } else if property == 11 {
        value.visibility[id] = text == "1"
        value.setVisible(id, text == "1")
        value.refreshEnablement()
    } else if property == 12 {
        value.enablement[id] = text == "1"
        value.refreshEnablement()
    } else if property == 2 {
        value.window.title = text
    } else if property == 3 {
        (value.nodes[id] as? NSButton)?.state = text == "1" ? .on : .off
    } else if property == 4, let slider = value.nodes[id] as? NSSlider,
        let data = text.data(using: .utf8), let config = try? JSONSerialization.jsonObject(with: data) as? [String: Double],
        let min = config["min"], let max = config["max"], min.isFinite, max.isFinite, min < max {
        slider.minValue = min
        slider.maxValue = max
    } else if property == 5, let combo = value.nodes[id] as? NSPopUpButton,
        let data = text.data(using: .utf8), let choices = try? JSONSerialization.jsonObject(with: data) as? [[String: String]] {
        combo.removeAllItems()
        for choice in choices {
            combo.addItem(withTitle: choice["label"] ?? "")
            combo.lastItem?.representedObject = choice["id"] ?? ""
        }
    } else if property == 6 {
        value.applying = true
        defer { value.applying = false }
        if let meter = value.nodes[id] as? NSProgressIndicator {
            meter.doubleValue = Double(text).map { $0.isFinite ? min(1.0, max(0.0, $0)) : 0.0 } ?? 0.0
        }
        if let slider = value.nodes[id] as? NSSlider, let number = Double(text) { slider.doubleValue = number }
        if let combo = value.nodes[id] as? NSPopUpButton {
            combo.select(combo.itemArray.first { ($0.representedObject as? String) == text })
        }
        if let tab = (value.nodes[id] as? Tabs)?.body {
            if let item = tab.tabViewItems.first(where: { ($0.identifier as? String) == text }) { tab.selectTabViewItem(item) }
            else if tab.numberOfTabViewItems > 0 {
                // Stale or empty selection (dynamic pages mounted after the
                // binding fired): fall back to the first page so the body
                // never stays blank.
                tab.selectTabViewItem(at: 0)
            }
        }
    } else if property == 7, let panel = value.dialogs[id] {
        if text == "1", !panel.isVisible {
            if value.window.isVisible { value.window.beginSheet(panel) } else { panel.makeKeyAndOrderFront(nil) }
        } else if text != "1" {
            if panel.sheetParent != nil { value.window.endSheet(panel) }
            panel.orderOut(nil)
        }
    } else if property == 8 { value.tabItems[id]?.identifier = text
    } else if property == 9 { value.dialogs[id]?.title = text
    } else if property == 10, let tabs = value.nodes[id] as? Tabs { tabs.setPosition(text)
    } else if let item = value.tabItems[id] {
        item.label = text
        if let tab = item.tabView, let id = tab.identifier?.rawValue, let tabs = value.nodes[id] as? Tabs {
            tabs.rebuildHeader()
        }
    } else if let field = value.nodes[id] as? NSTextField {
        if let editor = field.currentEditor() as? NSTextView, editor.hasMarkedText() { return }
        if field.stringValue != text { field.stringValue = text }
    } else if let button = value.nodes[id] as? NSButton { button.title = text }
    // Content may have changed above: re-resolve tab floors, throttled
    // (property sets stream at frame rate during play; floors only need to
    // track structural changes).
    let now = ProcessInfo.processInfo.systemUptime
    if now - value.lastFloorUpdate > 0.1 {
        value.lastFloorUpdate = now
        value.updateFloors()
    }
}

@_cdecl("w3_slider_value") @MainActor
public func sliderValue(_ pointer: UnsafeMutableRawPointer, _ idBytes: UnsafePointer<UInt8>?, _ idCount: Int,
                        _ output: UnsafeMutablePointer<Double>) -> Bool {
    guard let slider = host(pointer).nodes[string(idBytes, idCount)] as? NSSlider else { return false }
    output.pointee = slider.doubleValue
    return true
}

@_cdecl("w3_text") @MainActor
public func text(_ pointer: UnsafeMutableRawPointer, _ idBytes: UnsafePointer<UInt8>?, _ idCount: Int,
          _ output: UnsafeMutablePointer<UInt8>?, _ capacity: Int) -> Int {
    let value = host(pointer)
    let id = string(idBytes, idCount)
    let content: String
    if id == "__root" { content = value.window.title }
    else if let field = value.nodes[id] as? NSTextField { content = field.stringValue }
    else if let button = value.nodes[id] as? NSButton { content = button.title }
    else { return -1 }
    let bytes = Array(content.utf8)
    if let output = output, capacity >= bytes.count {
        bytes.withUnsafeBufferPointer { source in
            if let base = source.baseAddress { output.update(from: base, count: source.count) }
        }
    }
    return bytes.count
}

@_cdecl("w3_action") @MainActor
public func action(_ pointer: UnsafeMutableRawPointer, _ idBytes: UnsafePointer<UInt8>?, _ idCount: Int,
            _ textBytes: UnsafePointer<UInt8>?, _ textCount: Int, _ kind: Int32) -> Int32 {
    let value = host(pointer)
    let id = string(idBytes, idCount)
    if !value.interactive(id) { return 0 }
    if kind == 7, let slider = value.nodes[id] as? NSSlider, slider.isEnabled { return 1 }
    if kind == 0, let field = value.nodes[id] as? NSTextField, field.isEditable, field.isEnabled {
        field.stringValue = string(textBytes, textCount)
        value.controlTextDidChange(Notification(name: NSControl.textDidChangeNotification, object: field))
        return 1
    }
    if kind == 1, let button = value.nodes[id] as? NSButton, button.isEnabled {
        button.performClick(nil)
        return 1
    }
    if kind == 2, let item = value.menuItems[id], item.isEnabled,
       let menu = item.menu {
        menu.performActionForItem(at: menu.index(of: item))
        return 1
    }
    let text = string(textBytes, textCount)
    if kind == 3, let button = value.nodes[id] as? NSButton, button.isEnabled, text == "0" || text == "1" {
        button.state = text == "1" ? .on : .off
        value.activated(button)
        return 1
    }
    if kind == 4, let slider = value.nodes[id] as? NSSlider, slider.isEnabled,
       let number = Double(text), number.isFinite, (slider.minValue...slider.maxValue).contains(number) {
        slider.doubleValue = number
        value.activated(slider)
        return 1
    }
    if kind == 5, let combo = value.nodes[id] as? NSPopUpButton, combo.isEnabled,
       let item = combo.itemArray.first(where: { ($0.representedObject as? String) == text }) {
        combo.select(item)
        value.activated(combo)
        return 1
    }
    if kind == 5, let tabs = value.nodes[id] as? Tabs, !value.readonlyTabs.contains(id),
       let item = tabs.body.tabViewItems.first(where: { ($0.identifier as? String) == text }) {
        tabs.activatePage(tabs.body.indexOfTabViewItem(item))
        return 1
    }
    if kind == 6, value.dialogs[id] != nil, !value.readonlyDialogs.contains(id), text == "0" || text == "1" {
        value.emit(id, text)
        return 1
    }
    return 0
}

@_cdecl("w3_image") @MainActor
public func image(_ pointer: UnsafeMutableRawPointer, _ idBytes: UnsafePointer<UInt8>?, _ idCount: Int,
                  _ pixels: UnsafePointer<UInt8>?, _ length: Int, _ width: UInt32, _ height: UInt32) -> Int32 {
    guard let imageView = host(pointer).nodes[string(idBytes, idCount)] as? NSImageView else { return 0 }
    if width == 0, height == 0, length == 0 {
        imageView.image = nil
        if let picture = imageView as? PointerImage {
            picture.updateSourceSize(0, 0)
            let value = host(pointer)
            let id = string(idBytes, idCount)
            for constraint in picture.naturalDims { constraint.isActive = false }
            if var dims = value.dimensions[id] {
                dims.removeAll { old in picture.naturalDims.contains { $0 === old } }
                value.dimensions[id] = dims
            }
            picture.naturalDims = []
        }
        return 1
    }
    guard width > 0, height > 0, width <= 8192, height <= 8192,
          length >= 0, UInt64(width) * UInt64(height) * 4 == UInt64(length), length <= 64 * 1024 * 1024,
          let pixels = pixels,
          let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(width), pixelsHigh: Int(height),
              bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
              bitmapFormat: .alphaNonpremultiplied, bytesPerRow: Int(width) * 4, bitsPerPixel: 32),
          let destination = bitmap.bitmapData else { return 0 }
    destination.update(from: pixels, count: length)
    let image = NSImage(size: NSSize(width: Int(width), height: Int(height)))
    image.addRepresentation(bitmap)
    imageView.image = image
    if let picture = imageView as? PointerImage, !picture.fill {
        picture.updateSourceSize(Int(width), Int(height))
        // Data arriving after styling fills the natural axes with the
        // source size (explicit axes already hold exact style constraints).
        // Points, not pixels: the frame carries physical pixels while layout
        // runs in points, so an unscaled size feeds a window-size feedback
        // loop (capture tracks the window). Breakable priority, so content
        // never forces the window (no growth, no resize snap-back); small
        // images still render at source size with no required opposition.
        let value = host(pointer)
        let id = string(idBytes, idCount)
        let scale = max(1, value.window.backingScaleFactor)
        for constraint in picture.naturalDims { constraint.isActive = false }
        if var dims = value.dimensions[id] {
            dims.removeAll { old in picture.naturalDims.contains { $0 === old } }
            value.dimensions[id] = dims
        }
        picture.naturalDims = []
        var added: [NSLayoutConstraint] = []
        func natural(_ anchor: NSLayoutDimension, _ constant: CGFloat) {
            let constraint = anchor.constraint(equalToConstant: constant)
            constraint.priority = .defaultHigh
            added.append(constraint)
        }
        if let frame = value.frames[id] {
            let stretchWidth = picture.styleHorizontal == 0 || picture.styleHorizontal == 4
            if picture.styleWidth < 0 {
                if stretchWidth, value.directVerticalParent(of: id) {
                    natural(picture.widthAnchor, max(0, picture.sourceWidth / scale - CGFloat(picture.styleLeft + picture.styleRight)))
                    value.contentWidths[id]?.isActive = false
                } else if stretchWidth {
                    natural(frame.widthAnchor, picture.sourceWidth / scale)
                } else {
                    natural(picture.widthAnchor, max(0, picture.sourceWidth / scale - CGFloat(picture.styleLeft + picture.styleRight)))
                }
            }
            if picture.styleHeight < 0 {
                natural(frame.heightAnchor, picture.sourceHeight / scale)
            }
        }
        NSLayoutConstraint.activate(added)
        value.dimensions[id] = (value.dimensions[id] ?? []) + added
        picture.naturalDims = added
    }
    return 1
}

@_cdecl("w3_style") @MainActor
public func style(_ pointer: UnsafeMutableRawPointer, _ idBytes: UnsafePointer<UInt8>?, _ idCount: Int,
           _ top: Double, _ right: Double, _ bottom: Double, _ left: Double,
           _ width: Double, _ height: Double, _ horizontal: Int32, _ vertical: Int32,
           _ textRole: Int32, _ textScale: Double, _ red: Int32, _ green: Int32, _ blue: Int32) {
    let value = host(pointer)
    let id = string(idBytes, idCount)
    guard let frame = value.frames[id] else { return }
    // Viewport-filling tables track this in updateTableFloor; explicit
    // heights use the exact constraint path below instead.
    if let table = value.nodes[id] as? TableControl {
        table.fillsViewport = height < 0
    }
    if let label = value.nodes[id] as? NSTextField, !label.isEditable {
        let size = NSFont.systemFontSize * textScale
        switch textRole {
        case 3: label.font = NSFont.monospacedSystemFont(ofSize: size, weight: .regular)
        case 2: label.font = NSFont.boldSystemFont(ofSize: size * 1.2)
        case 4: label.font = NSFont.systemFont(ofSize: size, weight: .semibold)
        default: label.font = NSFont.systemFont(ofSize: size)
        }
        if red >= 0 && green >= 0 && blue >= 0 {
            label.textColor = NSColor(srgbRed: Double(red) / 255, green: Double(green) / 255, blue: Double(blue) / 255, alpha: 1)
        } else if textRole == 4 {
            label.textColor = NSColor.secondaryLabelColor
        } else {
            label.textColor = NSColor.labelColor
        }
    }
    if let constraints = value.dimensions[id] { NSLayoutConstraint.deactivate(constraints) }
    value.dimensions[id] = []
    // Manually laid out children (Grid/Flex/Absolute) take no stack position
    // constraints: the container assigns frames from placements and the sizes
    // recorded here. Margins still fold into the frame insets; the view keeps
    // filling its frame width like in stacks.
    if let parent = value.parents[id], value.isManualContainer(parent) {
        value.contentWidths[id]?.isActive = false
        value.contentHeights[id]?.isActive = false
        value.applyFrameInsets(id, top: top, right: right, bottom: bottom, left: left)
        value.recordManualSize(id, parent: parent, width: width, height: height)
        value.contentWidths[id]?.isActive = true
        return
    }
    value.contentWidths[id]?.isActive = false
    value.contentHeights[id]?.isActive = false
    value.applyFrameInsets(id, top: top, right: right, bottom: bottom, left: left)
    // Images stay centered (linkContent); style alignment must not reset it.
    if frame.orientation == .vertical {
        if !(value.nodes[id] is PointerImage) {
            switch horizontal {
            case 2: frame.alignment = .centerX
            case 3: frame.alignment = .trailing
            default: frame.alignment = .leading
            }
        }
        // End-aligned horizontal rows (dialog footers): the wrapper trailing
        // above positions a shrinked row, but to be robust when the wrapper
        // itself sizes to content, also make the row fill and push its
        // buttons right with a leading spacer. Button wrappers hug so only
        // the spacer stretches (otherwise .fill would stretch the buttons).
        if horizontal == 3, let row = value.nodes[id] as? NSStackView,
           row.orientation == .horizontal
        {
            row.distribution = .fill
            if value.spacers[id] == nil {
                let spacer = NSView()
                spacer.setContentHuggingPriority(.init(1), for: .horizontal)
                spacer.setContentCompressionResistancePriority(.init(1), for: .horizontal)
                spacer.translatesAutoresizingMaskIntoConstraints = false
                value.spacers[id] = spacer
                row.insertArrangedSubview(spacer, at: 0)
            }
            for sub in row.arrangedSubviews where sub !== value.spacers[id] {
                sub.setContentHuggingPriority(.init(750), for: .horizontal)
                sub.setContentCompressionResistancePriority(.init(750), for: .horizontal)
            }
            // Row must fill its wrapper for the spacer to absorb slack.
            value.contentWidths[id]?.isActive = true
        } else if horizontal != 3, let spacer = value.spacers[id],
                  let row = value.nodes[id] as? NSStackView,
                  row.arrangedSubviews.contains(spacer)
        {
            // Style changed away from End: remove the row spacer.
            row.removeArrangedSubview(spacer)
            spacer.removeFromSuperview()
            value.spacers.removeValue(forKey: id)
        }
    } else if horizontal == 3 {
        // End alignment on horizontal stacks means right-aligned children:
        // alignment only selects the cross axis here, so a flexible leading
        // spacer absorbs the slack under the default gravity distribution.
        // (Vertical stacks use .trailing above.)
        if value.spacers[id] == nil {
            let spacer = NSView()
            spacer.setContentHuggingPriority(.init(1), for: .horizontal)
            spacer.setContentCompressionResistancePriority(.init(1), for: .horizontal)
            spacer.translatesAutoresizingMaskIntoConstraints = false
            value.spacers[id] = spacer
            frame.insertArrangedSubview(spacer, at: 0)
        }
    } else if let spacer = value.spacers.removeValue(forKey: id) {
        frame.removeArrangedSubview(spacer)
        spacer.removeFromSuperview()
    }
    let stretchWidth = horizontal == 0 || horizontal == 4
        // End-aligned horizontal rows (footers) fill their wrapper so the
        // leading spacer can push buttons right. Safe (no exact width).
        || (horizontal == 3 && (value.nodes[id] as? NSStackView)?.orientation == .horizontal)
    // Stretch children absorb stack slack: NSStackView gravity distributes
    // leftover space inversely to hugging, so a Stretch child (e.g. a binding
    // value label that buttons must follow) needs the lowest hugging or it
    // shrinks to content and siblings ride its text width (jittering on
    // every value change). Exact widths still win via constraints.
    if horizontal == 4 {
        frame.setContentHuggingPriority(.init(1), for: .horizontal)
        // The content node fills the frame via an equality link: give it the
        // lowest hugging too, or the node's intrinsic width pins the frame.
        (value.nodes[id] as? NSView)?.setContentHuggingPriority(.init(1), for: .horizontal)
        // Gravity alone does not reliably yield to hugging here (the content
        // width link pins the frame to its content); .fill apportions by
        // hugging, so the Stretch child takes the slack while Start siblings
        // (default hugging) keep intrinsic size.
        if let parentId = value.parents[id],
           let stack = value.nodes[parentId] as? NSStackView,
           stack.orientation == .horizontal
        {
            stack.distribution = .fill
        }
    }
    if let node = value.nodes[id], node !== frame {
        let gravity: NSStackView.Gravity
        switch vertical {
        case 2: gravity = .center
        case 3: gravity = .bottom
        default: gravity = .top
        }
        frame.removeView(node)
        frame.addView(node, in: gravity)
        value.contentHeights[id]?.isActive = vertical == 4 || (vertical == 0 && (node is Tabs || node is Scrolling))
    }
    value.contentWidths[id]?.isActive = stretchWidth
    frame.setHuggingPriority(vertical == 4 ? NSLayoutConstraint.Priority(rawValue: 1) : .defaultLow, for: .vertical)
    // Images resolve each axis from the explicit placement, falling back
    // to the source size (spec: natural source size; aspect never grows).
    // The style tuple is stored so later image data can fill natural axes.
    var effWidth = width
    var effHeight = height
    if let image = value.nodes[id] as? PointerImage {
        image.styleWidth = width
        image.styleHeight = height
        image.styleHorizontal = horizontal
        image.styleLeft = left
        image.styleRight = right
        if width < 0, image.sourceWidth > 0 { effWidth = Double(image.sourceWidth) }
        if height < 0, image.sourceHeight > 0 { effHeight = Double(image.sourceHeight) }
    }
    var constraints: [NSLayoutConstraint] = []
    var widthConstraints: [NSLayoutConstraint] = []
    var heightConstraints: [NSLayoutConstraint] = []
    if effWidth >= 0 {
        if stretchWidth, value.directVerticalParent(of: id),
           let node = value.nodes[id], node !== frame {
            // Explicit widths in vertical stacks go on the content node,
            // not the frame: an exact frame width lets AppKit derive the
            // window width from content, snapping user resizes back to it
            // on every layout. The frame stays flexible (filling via stack
            // distribution like other flexible frames) while the content
            // holds its exact size; like the manual-container path, the
            // fill link is released since the node no longer fills it.
            widthConstraints.append(node.widthAnchor.constraint(equalToConstant: max(0, effWidth - left - right)))
            value.contentWidths[id]?.isActive = false
        } else if stretchWidth { widthConstraints.append(frame.widthAnchor.constraint(equalToConstant: effWidth)) }
        else if let node = value.nodes[id] { widthConstraints.append(node.widthAnchor.constraint(equalToConstant: max(0, effWidth - left - right))) }
    }
    if effHeight >= 0 { heightConstraints.append(frame.heightAnchor.constraint(equalToConstant: effHeight)) }
    else if vertical > 0, vertical < 4, !(value.nodes[id] is PointerImage),
              let container = frame.superview as? NSStackView, container.orientation == .horizontal {
        heightConstraints.append(frame.heightAnchor.constraint(equalTo: container.heightAnchor,
            constant: -(container.edgeInsets.top + container.edgeInsets.bottom)))
    }
    constraints = widthConstraints + heightConstraints
    if let image = value.nodes[id] as? PointerImage {
        image.naturalDims = (width < 0 && effWidth >= 0 ? widthConstraints : [])
            + (height < 0 && effHeight >= 0 ? heightConstraints : [])
    }
    NSLayoutConstraint.activate(constraints)
    value.dimensions[id] = constraints
}

@_cdecl("w3_remove") @MainActor
public func remove(_ pointer: UnsafeMutableRawPointer, _ idBytes: UnsafePointer<UInt8>?, _ idCount: Int) {
    host(pointer).remove(string(idBytes, idCount))
}

@_cdecl("w3_reorder") @MainActor
public func reorder(_ pointer: UnsafeMutableRawPointer, _ idBytes: UnsafePointer<UInt8>?, _ idCount: Int,
                    _ orderBytes: UnsafePointer<UInt8>?, _ orderCount: Int) -> Int32 {
    let value = host(pointer)
    let id = string(idBytes, idCount)
    guard let data = string(orderBytes, orderCount).data(using: .utf8),
          let ids = try? JSONSerialization.jsonObject(with: data) as? [String],
          Set(ids).count == ids.count else { return 0 }
    if let tabs = value.nodes[id] as? Tabs {
        let items = ids.compactMap { value.tabItems[$0] }
        guard items.count == ids.count, items.count == tabs.body.numberOfTabViewItems,
              items.allSatisfy({ $0.tabView === tabs.body }) else { return 0 }
        let selected = tabs.body.selectedTabViewItem
        let previousApplying = value.applying
        value.applying = true
        defer { value.applying = previousApplying }
        for (index, item) in items.enumerated() {
            if tabs.body.tabViewItem(at: index) !== item {
                tabs.body.removeTabViewItem(item)
                tabs.body.insertTabViewItem(item, at: index)
            }
        }
        if let selected = selected { tabs.body.selectTabViewItem(selected) }
        tabs.rebuildHeader()
        return 1
    }
    guard let rows = value.nodes[id] as? Rows,
          ids.count == rows.body.arrangedSubviews.count else { return 0 }
    let frames = ids.compactMap { value.frames[$0] }
    guard frames.count == ids.count, Set(ids).count == ids.count,
          frames.allSatisfy({ $0.superview === rows.body }) else { return 0 }
    for (index, frame) in frames.enumerated() {
        if rows.body.arrangedSubviews[index] !== frame {
            rows.body.insertArrangedSubview(frame, at: index)
        }
    }
    return 1
}

@_cdecl("w3_frame_height") @MainActor
public func frameHeight(_ pointer: UnsafeMutableRawPointer, _ idBytes: UnsafePointer<UInt8>?, _ idCount: Int) -> Double {
    let value = host(pointer)
    value.window.contentView?.layoutSubtreeIfNeeded()
    return value.frames[string(idBytes, idCount)].map { Double($0.frame.height) } ?? -1
}

@_cdecl("w3_rows") @MainActor
public func rows(_ pointer: UnsafeMutableRawPointer, _ idBytes: UnsafePointer<UInt8>?, _ idCount: Int,
          _ total: Int, _ start: Int, _ count: Int, _ height: Double) {
    let value = host(pointer)
    let id = string(idBytes, idCount)
    guard let rows = value.nodes[id] as? Rows else { return }
    rows.maximum = max(0, total - count)
    rows.start = start
    rows.rowHeight = height
    rows.fixedHeight?.isActive = false
    rows.fixedHeight = rows.heightAnchor.constraint(equalToConstant: Double(count) * height)
    rows.fixedHeight?.isActive = true
    rows.scroller.isEnabled = rows.maximum > 0
    rows.scroller.knobProportion = total > 0 ? min(1, Double(count) / Double(total)) : 1
    rows.scroller.doubleValue = rows.maximum > 0 ? Double(start) / Double(rows.maximum) : 0
}

func shortcut(_ value: String) -> (String, NSEvent.ModifierFlags) {
    let parts = value.split(separator: "+").map(String.init)
    var flags: NSEvent.ModifierFlags = []
    for modifier in parts.dropLast() {
        switch modifier {
        case "Cmd": flags.insert(.command)
        case "Ctrl": flags.insert(.control)
        case "Alt": flags.insert(.option)
        case "Shift": flags.insert(.shift)
        default: break
        }
    }
    let raw = parts.last ?? ""
    if raw.hasPrefix("Key") { return (String(raw.dropFirst(3)).lowercased(), flags) }
    if raw.hasPrefix("Digit") { return (String(raw.dropFirst(5)), flags) }
    let keys: [String: String] = ["Enter": "\r", "Tab": "\t", "Space": " ", "Escape": "\u{1b}",
        "Backspace": "\u{8}", "Delete": "\u{f728}", "Insert": "\u{f727}",
        "Home": "\u{f729}", "End": "\u{f72b}", "PageUp": "\u{f72c}", "PageDown": "\u{f72d}",
        "ArrowLeft": "\u{f702}", "ArrowRight": "\u{f703}", "ArrowUp": "\u{f700}", "ArrowDown": "\u{f701}"]
    if let key = keys[raw] { return (key, flags) }
    if raw.hasPrefix("F"), let number = Int(raw.dropFirst()), let scalar = UnicodeScalar(0xf703 + number) {
        return (String(scalar), flags)
    }
    return ("", flags)
}

@_cdecl("w3_menu") @MainActor
public func menu(_ pointer: UnsafeMutableRawPointer, _ idBytes: UnsafePointer<UInt8>?, _ idCount: Int,
          _ jsonBytes: UnsafePointer<UInt8>?, _ jsonCount: Int) {
    let value = host(pointer)
    let id = string(idBytes, idCount)
    guard let data = string(jsonBytes, jsonCount).data(using: .utf8),
          let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
          let title = object["title"] as? String, let items = object["items"] as? [[String: Any]] else { return }
    let submenu: NSMenu
    if let existing = value.menus[id] { submenu = existing }
    else {
        submenu = NSMenu(title: title)
        submenu.autoenablesItems = false
        let parent = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        parent.submenu = submenu
        if id == "app", let stub = value.bar.items.first {
            // The view owns the HIG application menu (About/Preferences/Quit
            // with real labels and shortcuts): it replaces the Host stub so
            // AppKit shows a single application menu. Hosts without an "app"
            // menu keep the stub.
            value.bar.insertItem(parent, at: 0)
            value.bar.removeItem(stub)
            value.ownsAppMenu = true
        } else {
            value.bar.addItem(parent)
        }
        value.menus[id] = submenu
    }
    submenu.title = title
    value.bar.items.first(where: { $0.submenu === submenu })?.title = title
    let old = value.menuItems.keys.filter { $0.hasPrefix(id + "/") }
    for key in old { value.menuItems.removeValue(forKey: key) }
    submenu.removeAllItems()
    for item in items {
        let role = item["role"] as? String ?? "Normal"
        if role == "Separator" { submenu.addItem(.separator()); continue }
        let key = id + "/" + (item["id"] as? String ?? "")
        let native = NSMenuItem(title: item["label"] as? String ?? "", action: #selector(Host.menuActivated(_:)), keyEquivalent: "")
        native.target = value
        native.representedObject = key
        native.isEnabled = item["enabled"] as? Bool ?? true
        let (equivalent, flags) = shortcut(item["shortcut"] as? String ?? "")
        native.keyEquivalent = equivalent
        native.keyEquivalentModifierMask = flags
        let selectors: [String: Selector] = ["Copy": #selector(NSText.copy(_:)), "Cut": #selector(NSText.cut(_:)),
            "Paste": #selector(NSText.paste(_:)), "SelectAll": #selector(NSText.selectAll(_:)),
            "About": #selector(NSApplication.orderFrontStandardAboutPanel(_:))]
        if let selector = selectors[role] { native.action = selector; native.target = nil }
        if role == "Quit" { native.action = #selector(ApplicationDelegate.quitRequested); native.target = ApplicationDelegate.shared }
        submenu.addItem(native)
        value.menuItems[key] = native
    }
    // Late menus (installed after the window already became key) still need
    // to take over the shared menubar; windows without menus never call here.
    if value.window.isKeyWindow { NSApp.mainMenu = value.bar }
}

@_cdecl("w3_menu_remove") @MainActor
public func menuRemove(_ pointer: UnsafeMutableRawPointer, _ idBytes: UnsafePointer<UInt8>?, _ idCount: Int) {
    let value = host(pointer)
    let id = string(idBytes, idCount)
    if let menu = value.menus.removeValue(forKey: id),
       let index = value.bar.items.firstIndex(where: { $0.submenu === menu }) {
        value.bar.removeItem(at: index)
    }
    let prefix = id + "/"
    for key in value.menuItems.keys.filter({ $0.hasPrefix(prefix) }) {
        value.menuItems.removeValue(forKey: key)
    }
}

@_cdecl("w3_menu_tree") @MainActor
public func menuTree(_ pointer: UnsafeMutableRawPointer, _ idBytes: UnsafePointer<UInt8>?, _ idCount: Int,
                     _ jsonBytes: UnsafePointer<UInt8>?, _ jsonCount: Int) {
    let value = host(pointer)
    let id = string(idBytes, idCount)
    guard let root = value.menus[id], let data = string(jsonBytes, jsonCount).data(using: .utf8),
          let tree = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] else { return }
    func build(_ menu: NSMenu, _ nodes: [[String: Any]]) {
        for node in nodes {
            if let children = node["children"] as? [[String: Any]] {
                let title = node["label"] as? String ?? ""
                let child = NSMenu(title: title)
                child.autoenablesItems = false
                let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
                item.isEnabled = node["enabled"] as? Bool ?? true
                item.submenu = child
                build(child, children)
                menu.addItem(item)
            } else if node["separator"] as? Bool == true { menu.addItem(.separator()) }
            else if let key = node["id"] as? String, let item = value.menuItems[id + "/" + key] {
                item.menu?.removeItem(item)
                menu.addItem(item)
            }
        }
    }
    root.removeAllItems()
    build(root, tree)
}

@_cdecl("w3_input_probe") @MainActor
public func inputProbe(_ pointer: UnsafeMutableRawPointer, _ idBytes: UnsafePointer<UInt8>?, _ idCount: Int) -> Int32 {
    let value = host(pointer)
    let id = string(idBytes, idCount)
    let pasteboard = NSPasteboard.general
    let saved = (pasteboard.pasteboardItems ?? []).map { original -> NSPasteboardItem in
        let copy = NSPasteboardItem()
        for type in original.types {
            if let data = original.data(forType: type) { copy.setData(data, forType: type) }
        }
        return copy
    }
    defer { pasteboard.clearContents(); if !saved.isEmpty { pasteboard.writeObjects(saved) } }
    guard let field = value.nodes[id] as? NSTextField, field.isEditable else { return -1 }
    value.window.makeFirstResponder(field)
    guard let editor = field.currentEditor() as? NSTextView else { return -2 }
    editor.selectAll(nil)
    editor.setMarkedText("kana", selectedRange: NSRange(location: 4, length: 0), replacementRange: NSRange(location: NSNotFound, length: 0))
    guard editor.hasMarkedText() else { return -3 }
    set(pointer, idBytes, idCount, nil, 0, 0)
    guard editor.hasMarkedText(), editor.string == "kana" else { return -4 }
    editor.insertText("\u{65e5}\u{672c}\u{8a9e}", replacementRange: editor.markedRange())
    guard !editor.hasMarkedText() else { return -5 }
    editor.selectAll(nil)
    editor.copy(nil)
    guard NSPasteboard.general.string(forType: .string) == "\u{65e5}\u{672c}\u{8a9e}" else { return -6 }
    editor.cut(nil)
    guard editor.string.isEmpty else { return -7 }
    editor.paste(nil)
    guard editor.string == "\u{65e5}\u{672c}\u{8a9e}" else { return -8 }
    guard field.accessibilityRole() == .textField, field.accessibilityIdentifier() == id else { return -9 }
    editor.insertTab(nil)
    guard let delegate = editor.delegate as? NSView, delegate !== field else { return -10 }
    editor.insertBacktab(nil)
    guard editor.delegate === field else { return -11 }
    return 1
}

@_cdecl("w3_editing_probe") @MainActor
public func editingProbe(_ pointer: UnsafeMutableRawPointer, _ idBytes: UnsafePointer<UInt8>?, _ idCount: Int, _ begin: Int32) -> Int32 {
    let value = host(pointer)
    guard let field = value.nodes[string(idBytes, idCount)] as? NSTextField else { return 0 }
    if begin != 0 {
        value.window.makeFirstResponder(field)
        guard let editor = field.currentEditor() as? NSTextView else { return 0 }
        value.editingProbe = editor
        value.editingProbeText = field.stringValue
        editor.selectAll(nil)
        editor.setMarkedText("kana", selectedRange: NSRange(location: 2, length: 1), replacementRange: NSRange(location: NSNotFound, length: 0))
        return editor.hasMarkedText() ? 1 : 0
    }
    guard let editor = field.currentEditor() as? NSTextView, editor === value.editingProbe,
          editor.hasMarkedText(), editor.string == "kana", editor.selectedRange() == NSRange(location: 2, length: 1) else { return 0 }
    editor.setMarkedText(value.editingProbeText, selectedRange: NSRange(location: 0, length: 0), replacementRange: editor.markedRange())
    editor.unmarkText()
    value.editingProbe = nil
    return 1
}

@_cdecl("w3_post_key") @MainActor
public func postKey(_ pointer: UnsafeMutableRawPointer, _ idBytes: UnsafePointer<UInt8>?, _ idCount: Int) -> Int32 {
    let value = host(pointer)
    guard let field = value.nodes[string(idBytes, idCount)] as? NSTextField,
          value.window.makeFirstResponder(field),
          let event = NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [],
            timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: value.window.windowNumber,
            context: nil, characters: "x", charactersIgnoringModifiers: "x", isARepeat: false, keyCode: 7) else { return 0 }
    NSApp.postEvent(event, atStart: false)
    return 1
}

@_cdecl("w3_shortcut") @MainActor
public func performShortcut(_ pointer: UnsafeMutableRawPointer, _ bytes: UnsafePointer<UInt8>?, _ count: Int) -> Int32 {
    let value = host(pointer)
    let (key, flags) = shortcut(string(bytes, count))
    guard let event = NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: flags,
        timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: value.window.windowNumber,
        context: nil, characters: key, charactersIgnoringModifiers: key, isARepeat: false, keyCode: 0) else { return 0 }
    return NSApp.mainMenu?.performKeyEquivalent(with: event) == true ? 1 : 0
}

@_cdecl("w3_capture") @MainActor
public func capture(_ pointer: UnsafeMutableRawPointer, _ pathBytes: UnsafePointer<UInt8>?, _ pathCount: Int, _ width: Double) -> Int32 {
    let value = host(pointer)
    value.window.setContentSize(NSSize(width: width, height: 660))
    value.window.displayIfNeeded()
    guard let content = value.window.contentView,
          let bitmap = content.bitmapImageRepForCachingDisplay(in: content.bounds) else { return 0 }
    content.cacheDisplay(in: content.bounds, to: bitmap)
    guard let data = bitmap.representation(using: .png, properties: [:]) else { return 0 }
    do { try data.write(to: URL(fileURLWithPath: string(pathBytes, pathCount))); return 1 }
    catch { return 0 }
}
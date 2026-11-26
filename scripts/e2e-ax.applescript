-- AT-SPI equivalent for macOS: OS-observed layout via System Events.
-- Usage (interactive macOS desktop, Accessibility permission granted):
--   osascript scripts/e2e-ax.applescript "layout golden"
-- Prints window-relative id/position/size lines for manual diff against
-- `dump_layout_json`. Window screen position is subtracted so only
-- window-relative geometry is compared. Exact match required.
on run argv
  set windowTitle to "layout golden"
  if (count of argv) > 0 then set windowTitle to item 1 of argv
  tell application "System Events"
    set targetProcess to first process whose background only is false and name contains "panechord"
    set targetWindow to first window of targetProcess whose title contains windowTitle
    set windowPos to position of targetWindow
    set windowX to item 1 of windowPos
    set windowY to item 2 of windowPos
    repeat with uiElement in entire contents of targetWindow
      try
        set elementPos to position of uiElement
        set elementSize to size of uiElement
        log ("id=" & (description of uiElement) & " x=" & ((item 1 of elementPos) - windowX) & " y=" & ((item 2 of elementPos) - windowY) & " w=" & (item 1 of elementSize) & " h=" & (item 2 of elementSize))
      end try
    end repeat
  end tell
end run

# Surface bridge for macOS

Owns the private GL drawable host shared by the GTK (`../gtk`) and Tao
(`../tao`) backends on macOS. This crate is the single home for the
hidden-subview dance, so the platform SDK (`objc2`) stays out of
`panechord-runtime`: the runtime core never names backend or platform
vocabulary (same DIP as the shortcut renderers).

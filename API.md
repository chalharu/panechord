# Panechord implementation contract

Panechord is a standalone workspace. The public entry is the `panechord`
facade crate, which re-exports the runtime and the declarative macros;
backends are selected per application via the `native`/`gtk`/`tao`/`macos`
features. The practical screens in this workspace are `panechord-demo`
and `panechord-library`.

Downstream crates depend only on the facade, never on the concrete backend
crates. Declarative screens name the macro backend path through the
facade-owned `panechord::backend` module (one alias per feature, precedence
tao > gtk > macos > native, mirroring `Runner`): e.g.
`panechord::view!("ui/main.yaml", model, panechord::backend)`. Depending on
`panechord-gtk`/`panechord-tao`/… directly to name the backend is a layering
violation; native backend selection belongs to Panechord.

## Runtime API shared with codegen/backends

- `Executor::spawn(LocalBoxFuture<'static, ()>)` (UI local).
- `Binding<T>`: `get`, `set -> bool`, `try_set -> Result<(),WriteError>`,
  `is_writable`, `subscribe(Rc<dyn Fn(T)>) -> Subscription`, `constant`, `map`
  (read-only), `distinct` (requires PartialEq), `convert(forward,backward)`
  (fallible backward `Result<T,String>`).
- `WriteError { ReadOnly, Closed, Capacity, Conversion(String) }`: typed
  rejection, not an application receipt. Acceptance bools are must_use.
- `Command { pub enabled: Binding<bool> }`, `invoke()->bool`.
- `Store<S:Clone+'static>`: Clone, `new(state, Rc<dyn Executor>)`, `snapshot`,
  `update(impl FnOnce(&mut S)+'static)->bool`, `close`, `is_closed`.
- `Store::with_dispatch_budget(state, executor, Limits, NonZeroUsize)`: opt-in
  event budget per drain, with weak yielding continuations. Accepted writes can
  remain queued on return. FIFO and capacity remain unchanged; host scheduling,
  not this event count, determines OS fairness. Existing constructors remain
  synchronous.
- `Store::bind<T:Clone+PartialEq>(read: impl Fn(&S)->T+'static,
  write:Option<fn(&mut S,T)>)->Binding<T>`.
- `Store::try_into_owner(self)->Result<StoreOwner<S>,Self>`: acquires a
  non-Clone lifecycle authority only before sharing. Failure returns the
  original Store without closing it. Owner close/drop closes its Store;
  `store()` exposes the shared handle. `Store::close` remains an explicit
  force-close operation.
- `bind_scoped_explicit` / `update_scoped_explicit`: advanced dependency
  contracts. Every changed top-level field must be correct and complete. Codegen
  uses the former with read/write/dependency generated from the same parsed
  field path.
- `Store::watch<T:Clone+'static>(source:Binding<T>, react:impl
  Fn(&Store<S>,T)+'static)->Subscription`: latest-value reaction through bounded
  FIFO after publication, retries after saturation with positive ingress.
  Initial reaction may be synchronous. Token drop invalidates queued reactions,
  not already-started effects. Each reaction begins with one ingress slot free;
  further submissions must check acceptance. Tokens do not own Store; exclude
  outputs from sources to avoid feedback.
- `Store::command<A:Clone+'static>(enabled: impl Fn(&S)->bool+'static, action:
  impl Fn(&Store<S>,A)+'static, argument:Binding<A>)->Command`. Recheck enabled
  on dequeue. Argument evaluated at invocation; the native-facing Command erases
  its argument type.
- `Store::effect(policy:Policy, slot:&str, future:impl Future<Output=R>+'static,
  apply:impl FnOnce(&mut S,R)+'static)->bool`. Policy Latest/Serial/Parallel.
  Futures outside borrow, bounded ingress/tasks, weak ownership, stale lifecycle
  protection.
- `validation(&Store<S>,source:Binding<String>,error:Binding<String>,pending:
  Binding<bool>,validator:impl
  Fn(String)->LocalBoxFuture<'static,Result<(),String>>+'static)->Subscription`:
  Latest per subscription, initial validation, no stale result on unmount.
- `Keyed<S> { pub key:String, pub model:Store<S> }`: Clone/PartialEq by key and
  store identity. `Keyed::new(key:impl Into<String>, model:Store<S>)`.
- `Viewport { pub start:usize, pub count:usize, pub row_height:f32 }`:
  Clone/Copy/PartialEq; default start0/count20/row_height48.0.
  `validate()->Result<Self,&'static str>`,
  `range(length)->Result<Range<usize>,&'static str>`.
- `Edges {top:f32,right:f32,bottom:f32,left:f32}`: Default/Clone/Copy/PartialEq.
- `Placement
  {row:i32,column:i32,row_span:i32,column_span:i32,x:f32,y:f32,
  width:Option<f32>,height:Option<f32>}`
  Default spans1.
- `Style {margin:Edges,padding:Edges,placement:Placement}`
  Default/Clone/Copy/PartialEq.
- `Layout { Stack {horizontal:bool,gap:f32}, Grid {columns:u32,gap:f32}, Flex
  {gap:f32}, Absolute }` Clone/Copy/PartialEq.
- `MenuRole {Normal,Copy,Cut,Paste,SelectAll,About,Quit,Separator}`
  Clone/PartialEq.
- `MenuItem<A=String> {
  id:String,label:String,argument:A,shortcut:Option<String>,role:MenuRole,
  enabled:bool
  }` Clone/PartialEq when A supports them.
- `Shortcut::parse` accepts shared shortcut syntax, including `Primary+S` and
  `Primary+Q`. Each backend lowers the parsed form with its own renderer
  (`panechord_gtk::shortcut::to_gtk`, `panechord_tao::shortcut::to_muda`,
  `panechord_macos::shortcut::to_bridge`); the runtime only parses and exposes
  `modifiers()`/`key()`, never backend vocabulary. Primary is Cmd on macOS and
  Ctrl elsewhere. Anything outside the shared grammar is rejected.

Reads and initial subscriptions project from a retained `Rc<S>` outside internal
borrows, without cloning all of S. `snapshot()` still returns an owned clone;
legacy mutation clones S once and invalidates all projections. Explicit scoped
binding
writes only evaluate that field's subscriptions plus unscoped subscribers, but
still clone S. `update_scoped_explicit` avoids the state clone and indexes dirty
fields;
use Rc/Arc for structural sharing of unchanged large data. A wrong dirty-field
declaration can suppress required notifications. No automatic arbitrary-closure
dependency inference or general O(1) update guarantee is claimed.
If an executor discards a dispatch continuation, the Store fails closed and
`dispatch_failed()` is true. Executors must service accepted futures while live.
This detects cancellation/drop, not a future that is retained forever without
polling.

### Common viewport contract

GTK, Tao and AppKit use the runtime validator: count must be 1..=256 and
row_height must be finite and 1..=4096 logical pixels. No count/height clamping
or NaN fallback. The proc macro validates literals with the same function;
dynamic bindings are checked before changing the accepted viewport or mounts.
An invalid initial viewport mounts nothing; an invalid update retains existing
mounts and reports an error. A subsequent valid update resumes normal display.
Backend error APIs retain their existing conventions (Tao diagnostics are a
log).

`range(length)` returns at most count rows, normalizing an out-of-range start to
`length.saturating_sub(count)`, so the last page is full unless the source is
shorter than count. Empty sources give 0..0; usize overflow is avoided. This
does
not promise to write the normalized start back into a read-only binding.
Writable viewport bindings receive the effective normalized start on all
backends;
read-only bindings retain the requested model value and scroll locally.
Tao may mount one extra trailing row during fractional native scrolling (257
maximum); GTK/AppKit do not overscan. Native sizes and scroll rounding remain
backend-specific. This does not unify other layout/style/menu error policies.

## Identical GTK/Tao View API

`View` Clone, `Parent` Clone. `run_host(make: impl
FnOnce(Rc<Host>)->Result<(),String>)->Result<(),String>` owns the single native
loop; screens build inside `make` and present via `register_view`/`present`.

### Shared capability traits and backend abstraction

The three concrete `View` types each satisfied the whole screen API, so every
client depended on every method. The shared contract is now capability traits
in `panechord_runtime` — `ViewCore`, `LayoutView`, `TextView`, `ButtonView`,
`SelectView`, `MediaView`, `TabView`, `ListView`, `TableView`, `DialogView`,
`MenuView`, `AutomationView` — with
`View` as the umbrella (blanket-impl'd) and `WeakViewHandle` for weak handles.
Generic code bounds on what it uses (`fn show<V: TextView + ButtonView>`);
codegen asserts `V: View`. Each backend implements all twelve plus its own
inherent extensions (`mount_native`/`bind_native`, Tao `mount`/`mount_bound`,
AppKit `surface`/probes): extension without touching the contract.

System theme (dark mode, high contrast) is inherited, not overridden: default
text, button and progress styling comes from the native theme on every
backend (GTK theme classes, iced theme, AppKit system colors). An explicit
`foreground` in `Style`/table cells is an application override and stays
exactly as specified, so contrast for custom colors is the application's
responsibility.

`panechord_runtime::Backend` is the host-side shape (`run_host`, option
setters; `View`/`Host` assoc types with
`Host: WindowHost<View>`). `panechord::Runner` drives the feature-selected
backend through the trait, so new backends/options extend the trait instead of
editing dispatch. All three `GtkHost`/`TaoHost`/`MacHost` implement
`WindowHost<View>`; all `WeakView`s implement `WeakViewHandle`.

Teardown lifecycle (`panechord_runtime::Lifecycle`: closed flag, close hooks,
request handler, reentrancy gate) is a shared runtime component, not fields
mixed into the content registries nor triplicated per backend; host
coordination slots stay in `Inner`. Generated
screens construct through `<Backend as panechord_runtime::Backend>::View::new`
and import the capability traits, so codegen depends on the contract, not on
backend concretions (`RuntimePaths` rewrites the runtime path for
facade/renamed-dependency builds, including `use` prefixes).

- `View::new(title:Binding<String>)->Self`, `root()->Parent`.
- `layout(&Parent,id:&str,layout:Layout)->Parent`; `try_layout` returns
  `Result<Parent,String>` and rejects invalid sizes, unavailable/foreign parents
  and duplicate IDs before mutation. The infallible wrapper panics on rejection.
- `style(id:&str,Style)` applies leaf or container spacing/placement (root
  layout included).
- `try_style(id:&str,Style)->Result<(),String>` validates before applying.
  Closed/unknown targets and invalid spacing/placement reject without mutation.
  Root is `__root`, never an arbitrary missing-ID alias. `style` records the
  same error through the backend error channel.
- `label/entry(&Parent,id:&str,text:Binding<String>)`.
- `button(&Parent,id:&str,text:Binding<String>,command:Command)`.
- `meter(&Parent,id:&str,value:Binding<f64>)` native/custom primitive demo.
- `checkbox(parent,id,text:Binding<String>,checked:Binding<bool>)` and
  `radio(parent,id,text,selected:Binding<String>,value:&str)`; Radio selects a
  stable key, CheckBox click toggles.
- `slider(parent,id,value:Binding<f64>,SliderRange {min,max,step})`; finite
  range, positive step, nearest-step normalization. Invalid source values retain
  the last accepted display; invalid automation rejects.
- `combo(parent,id,items:Binding<Vec<Choice>>,selected:Binding<String>)`;
  `Choice {id,label}`, unique nonempty slash-free IDs, at most 4096. Empty
  selection means no selected item.
- `image(parent,id,source:Binding<Option<ImageFrame>>)`; validated
  straight-alpha RGBA8, dimensions 1..=8192, at most 64 MiB. ImageFrame shares
  Arc bytes; equality is frame identity plus dimensions, not a full pixel
  comparison. None clears the image. Native image minimum/allocation differs
  from iced fixed frames.
- `tab(parent,id,selected)->Parent`, `tab_page(parent,id,title,key)->Parent`,
  `finish_tab(parent)->Result<(),String>`; 1..=256 unique pages. Codegen calls
  finish_tab after building pages. Switching retains content and subscriptions,
  invalid selection retains the accepted display. Empty selection displays the
  first page.
- `tab_position(id,TabPosition)->Result<(),String>` changes header placement
  without rebuilding pages. `TabPosition::{Top,Bottom,Left,Right}` defaults to
  Top, preserving `tab()` behavior. Left/Right use a vertical header rail; all
  backends scroll overflowing vertical headers. Closed, missing and non-Tab
  targets reject. Selected bindings retain their readonly contract independently
  of placement.
- `dialog(parent,id,title:Binding<String>,open:Binding<bool>)->Parent`; native
  modal GTK/AppKit window or Tao input-blocking overlay. Does not block the
  executor; readonly open rejects automation/close changes. HIG Return/Escape:
  inside an open dialog, Return activates a non-destructive enabled accept and
  Escape activates cancel; a destructive accept is never a Return target.
  Footers outside dialogs keep no key wiring.
- automation `toggle(id,bool)`, `slide(id,f64)`, `select(id,key)`,
  `set_open(id,bool)` return `Result<(),String>`. Closed, missing, readonly or
  invalid operations reject. Native delivery/acceptance is not an eventual
  application receipt.
- `items<T:Clone+'static>(&Parent,id:&str,items:Binding<Vec<Keyed<T>>>,
  viewport:Binding<Viewport>,template:Rc<dyn Fn(&View,&Parent,&str,Store<T>)>)`.
  Compile arbitrary recursive template body, not string rendering. Stable row
  scope IDs `{listid}/{key}/{localid}`. Duplicate keys rejected before changes.
  Native scrolling adjusts mounted range. `viewport(id,start)` automation. Drop
  mounted widget subscriptions on eviction; item Store persists in source.
- `content<T:Clone+'static>(&Parent,id:&str,item:Binding<Option<Keyed<T>>>,
  template:Rc<dyn Fn(&View,&Parent,&str,Store<T>)>)`.
- `menu(id:&str,title:Binding<String>,items:Binding<Vec<MenuItem>>,dispatch:
  Rc<dyn Fn(String)->bool>)` dynamic snapshots, shortcuts and native roles.
- `typed_menu<A:Clone+'static>(id,title,items:Binding<Vec<MenuItem<A>>>,
  dispatch:Rc<dyn Fn(A)->bool>)`: typed arguments retained in Rust closures.
  Codegen uses this API with `A=String` for flat items.
- `keep(Subscription)`, `on_close(Rc<dyn Fn()>)`, `close`, `present`.
- `downgrade()->WeakView`, `WeakView::upgrade()->Option<View>`; closed views
  cannot be upgraded.
- `on_close_request(Rc<dyn Fn()>)` replaces the application's request handler;
  `request_close()` requests permission to close. Synchronous recursive requests
  are ignored. WM close and Quit route through this entry. `close()` is
  explicitly forceful and idempotent; `on_close` runs final cleanup, not
  confirmation.
- automation `edit`, `click`, `activate_menu` -> Result<(),String>;
  `text(id)->Option<String>`, `item_keys(id)->Vec<String>` (mounted keys),
  `title()->String`, `mounted_count(id)->usize`, `viewport(id,start)`.
- `native_*` extension factory is an application function called by generated
  code, e.g. `crate::meter(view,parent,id,binding)` calling backend `meter`; GTK
  must expose a genuine custom-widget mounting API for external primitive
  construction; Tao can mount custom iced Element factory. Neither should
  misrepresent software widgets as OS native.

### Adding a backend

A new backend is one crate plus facade wiring. No existing file changes
shape; the trait gains required methods instead of new dispatch arms, so the
compiler forces every backend to implement them.

1. Crate `panechord-<name>`: register it in the workspace
   (`[workspace.members]`,
   `[workspace.dependencies]` in the root `Cargo.toml`) and as an optional
   dependency plus feature in `panechord/Cargo.toml`.
2. `View`: implement all twelve capability traits (`ViewCore`, `LayoutView`,
   `TextView`, `ButtonView`, `SelectView`, `MediaView`, `TabView`, `ListView`,
   `TableView`, `DialogView`, `MenuView`, `AutomationView`). The umbrella
   `View` is blanket-impl'd, so satisfying the twelve is sufficient. Provide
   the `Parent` mount handle, `WeakView: WeakViewHandle`, and read-only
   probes (`text`, `item_keys`, `mounted_count`, `title`).
3. `Host`: implement `UiHost` (`executor`, single-`Session` `attach`) and
   `WindowHost::register_view_with_hooks`. The plain `register_view` default
   is the only public registration shape; do not add a host-level `present`
   (examples register through `register_view`, never around it).
4. `Executor`: implement `spawn`. Override `dispatch_budget` only for a
   cooperative single-thread loop (AppKit returns 64); preemptive loop
   integrations keep the default unbounded.
5. Backend surface the facade names only through `backend`: `run_host`,
   `pick_file`, `set_allows_automatic_window_tabbing`,
   `set_initial_window_size`, plus the `Backend` struct
   (see `gtk/src/view.rs`) with a short `NAME` for logs. No-op
   the setters where the platform has no equivalent.
6. Facade (`panechord/src/lib.rs`): extend the `compile_error!` exclusivity
   guard, add `pub use panechord-<name> as <name>` and the `as backend`
   alias. No per-symbol re-exports: concrete `View`/`Parent`/`WeakView`
   resolve through `panechord::backend` only. `SelectedBackend` and
   `Runner` follow automatically; screens keep using `panechord::backend`
   and `Backend::NAME`, never per-file backend aliases.
7. Consumers: add `backend-<name> = ["panechord/<feature>"]` to
   `demo/Cargo.toml` and `library/Cargo.toml`. Shared screens must build
   under every backend with zero `cfg(feature)`; backend-only measurement
   stays in the backend's own examples.
8. HIG conformance, owned by the framework, not copied per backend: footer
   order via `DialogFooter::ordered()`; Quit through the vetoable
   close-request path, never a predefined quit; Return/Escape dialog-scoped
   with destructive accept never a Return target; `Primary` modifiers through
   the runtime `Shortcut` parser.
9. Evidence: an example mirroring `tao/examples/multiwindow.rs`
   (registration, reopen, foreign/closed rejection) plus headless tests for
   the contract behaviors above, and a `<name>/README.md` stating backend
   behavior and limits. Inherent extensions are allowed but shared screens
   cannot use them — if every screen needs it, it belongs in the shared
   traits.

### Shared layout behavior

Vertical Stack children fill the available width after margins unless explicitly
sized. Horizontal Stack distributes surplus width among non-explicit children,
rather than leaving all buttons at their text width. Tao bounds Fill siblings
during natural-width measurement so an entry or meter cannot monopolize a row.
Native font metrics and minimum sizes still differ; equal layout does not mean
equal pixels or identical text wrapping. Labels and button captions can wrap
long
words; button captions are centered.

GTK/Tao window content scrolls vertically when it exceeds the viewport; the
native
menu remains outside that scroll area. An edge clipped while scrolling is not a
missing control. Explicit placements and fixed virtual row heights remain caller
contracts: the complete template, including padding and wrapped text, must fit
`row_height`. Fixed rows are not automatically enlarged by the window scroller.

### Unsized layout priority

When no explicit placement size is given, backends resolve sizes in this
order (highest first):

1. Window bounds win. Content never resizes the window; the initial size
   and user resizes rule. A backend that shrink-wraps or snap-backs the
   window to content size is in violation.
2. Explicit `placement.width`/`height` hold as caller contracts, per axis.
   With a single axis given, the other axis resolves via the rules below.
3. Overflow scrolls, never clips silently and never grows the window.
   Every backend must keep overflowing content reachable through a scroll
   container (macOS outer document scroll; GTK/Tao inner Scroll nodes,
   including a root scroll when the document exceeds the viewport).
4. Natural sizes: text, buttons, tab headers/rails, and image source
   dimensions. An Image with no placement renders at its source size,
   centered in its row. Narrowing never squeezes images below source:
   image extent floors the window minimum on every backend instead.
5. Fill: vertical Stack children fill the available width, except Images
   which keep their source width centered; horizontal Stack shares surplus
   width among non-explicit children; Tao bounds Fill siblings during
   natural-width measurement as before.
6. Derived sizes last: Image aspect only relates already-determined
   dimensions. It may shrink rendering to fit (letterbox) but must never
   grow content, invent pixels, or move the window. Concretely, an
   explicit height suspends aspect tracking of the width (and vice
   versa); with neither axis given, the source size stands.

## Codegen input

### macOS practical-screen subset

`panechord-macos` supplies `View`/`Parent`/`run_host` for the settings and
library screens,
with AppKit Stack, Grid, Flex, Absolute, Label, TextBox, Button, meter,
ItemsControl, Menu and the seven required
controls. It uses the same
code generation and existing application wiring; no runtime markup parser is
added.
The backend's internal build.rs compiles a Swift C ABI bridge. Custom `Native`
factories call `View::meter` like the other backends; ContentControl
is supported. `MacHost`
implements UiHost; `run_host(FnOnce(Rc<MacHost>)->Result<(),String>)` drives one
app-wide loop. `register_view(&Session,id,&View,WindowHooks)` retains the
window,
keeps its registration in the View scope, and routes app Quit to the Session.
MacHost accepts one Session identity for its lifetime. Reattaching a clone is
idempotent; attaching or registering a window with another Session returns an
error before registering or presenting it. `register_view` attaches the first
Session automatically. `Session::ptr_eq(&other)` compares coordinator identity.
Window close remains per-View. App-wide delegate is shared; native menus switch
with the active Window. Hooks explicitly own confirmation/cancel/drain/close;
closing shared Views does not implicitly close their Store. Style remains
limited.
Confirmation has no deadline; only draining after all approvals is subject to
the MacHost five-second timeout. Explicit cancellation/force-close remain
separate.
See [macos/README.md](macos/README.md) for supported input paths, native probes,
dispatch budgets, development-only dylib layout and the manual IME/VoiceOver
gates.

AppKit retains an accepted local viewport, so read-only/literal viewports still
allow scrolling without writing to the model. `item_keys` returns mounted keys,
not the complete source. Accepted row-height changes resize existing mounts.
Eviction releases nested list registries, viewport handlers and scoped tokens.
During Template construction `keep` belongs to the current row scope, restored
after nested construction or unwinding. Eviction drops validation tokens, aborts
their pending work and resets pending; remounting does not accumulate
subscriptions.
Menu snapshots validate all IDs and shortcuts before replacing native items and
handlers; rejection retains the old menu. `menu_error(id)->Option<String>`
reports
the per-menu rejection and clears on a valid update, without terminating the
host.
AppKit automation `frame_height(id)->Option<f64>` reads the native layout frame
after layout, returning None for absent controls or a closed View.
Rejected item snapshots retain the accepted source for subsequent scrolling.
Native close, viewport and ordinary actions have separate ABI event kinds, not
reserved strings in the control-ID namespace.

`panechord_macros::view!("ui/main.yaml", model, backend)` returns a constructed
shared
View. It retains the model until View close/drop but does not close a model with
other owners. `owned_view!` consumes a StoreOwner, not a Store or its clone.
Acquire it before creating shared handles; an existing shared View prevents
acquisition.
Sharing a handle obtained from an already acquired owner explicitly delegates
final
lifecycle authority to that owner. Owner drop/close closes those later handles
too.
Late `on_close` registration runs immediately on an already closed View.
include_str! tracks Cargo inputs; all Rust is emitted in expansion, not OUT_DIR.

`#[panechord_macros::commands]` creates an extension trait implemented by
`Store<Model>`.
`#[command] fn add(&mut self, argument: A)` generates `command_add(Binding<A>)`;
static commands accept `(Store<Self>, A)`. A must be Clone + 'static. Receiver
actions queue update; static actions receive Store outside a state borrow.
Noargument receiver methods keep their legacy `Binding<String>` signature.
Import the generated extension trait at the view call site. Generic models
remain unsupported.

In generated argument signatures, `Self` is recursively normalized to the source
model, including nested types, qualified associated types and associated const
expressions. It never changes meaning to `Store<Model>`. Source signatures stay
unchanged. Explicit `Self` inside opaque type macros is rejected with a
diagnostic;
use the concrete model type there.

Markup schema v2: root Window {id,title,content,menus:[...]}; layout children +
margin/padding + grid/absolute placement. text properties `{literal:...}` /
`{bind:{path:name,mode:TwoWay,converter:crate::converter}}`. Converter path has
`.forward(value)`/`.backward(value)->Result<_,String>` associated functions or
simple runtime built-in. Macro emits field closures, Rust enforces field
type/unknown names. Labels OneWay; Entry strips writer for OneWay.

`templates: {row: {root: <any Control>}}`; ItemsControl
{id,items:{bind:{path:rows}},item_template:row,viewport:{bind:{path:viewport}}};
ContentControl {id,content:{bind:{path:selected}},content_template:row}. Inner
template `path:name` resolves item Store; `root.name` resolves outer Store. IDs
automatically scope, no manual input enums/descriptors.

Menu {id,title,items:{bind:{path:menu_items}},command:choose}. Native
{id,factory:crate::meter,value:{bind:{path:progress}}}. Validation on TextBox
`validation:
{validator:crate::validate_name,error:name_error,pending:validating}`.

YAML nodes retain parser-derived line/column and template scope. Stable Rust
cannot create proc-macro spans in an arbitrary external YAML file. Use
`sh scripts/check-yaml.sh check ...` (Python 3) to translate node expansion
diagnostics into YAML primary spans in rustc JSON and rendered output; original
Rust spans, errors and exit status remain. Formal gates enable this wrapper.
Without it rustc still checks types, but the primary location can remain view!.
Downstream probes assert YAML lines for typo, type mismatch, unknown command,
factory input/return, validator input, converter and typed Menu mismatch.
JSON nodes also retain source spans; downstream Cargo UI checks assert an
external
JSON primary span after rustc type rejection. This is not an editor LSP. Stable
Rust still needs the diagnostic wrapper to replace arbitrary external-file
spans.

`try_view!` and `try_owned_view!` return `Result<View, String>` for reported
initial
construction errors. Failure closes the partial View; the owning variant also
closes its consumed StoreOwner, while a shared Store remains open. These are not
catch-all wrappers for arbitrary panics in application callbacks or native
allocation.
Generated layouts/pages use `try_layout`/`try_tab_page`; literal Tab selections
must be empty or name a declared page. Late template errors remain View
diagnostics.

All controls accept optional OneWay `visible`, `enabled` and `style_binding`.
Visibility retains the mounted control; enablement also respects ancestors and
does not restore a readonly writer or override Command enablement.
`style_binding`
is a full `Binding<Style>` replacement, validated before mutation, not a partial
CSS patch. Window creation/closure still belongs to the typed host.

Menu declarations with `nested: true` bind `Vec<MenuNode<A>>` and call
`hierarchical_menu`. Submenus carry stable IDs, labels, enablement and children;
leaf items retain typed arguments and shortcuts. Whole-tree validation preserves
the last accepted snapshot. Limits are 64 levels and 4096 nodes with unique IDs.
Existing flat `MenuItem<A>` and `typed_menu` remain supported.

Numeric literals remain f64 by default. Command arguments may explicitly use
`{literal: {i32: 7}}`, `{literal: {i64: -7}}` or `{literal: {u64: 7}}`;
out-of-range or noninteger payloads reject. Omitted String arguments preserve
the
legacy empty value; other argument types receive an explicit required-argument
diagnostic.

Tao `run_host` provides a single-loop `TaoHost` for independent native windows.
Use `UiHost::executor`, `register_view(session,id,view,hooks)` and `present`;
closing a window does not close shared models or the scheduler while another
window survives. Foreign Session/closed View registrations reject, and IDs can
be reused only after their prior registration closes. External reactor
attachment
without this owned host is still unsupported.

`slide` reports normalized write acceptance, not eventual application. Closed,
capacity, conversion, readonly, hidden and disabled inputs reject without false
display changes. `slider_value` exposes the retained numeric display, not raw
model data; it returns None after View closure. `image_update_count` counts
image
setter/Handle creation, not compositor presentations or hardware uploads.
Shutdown hook panics are recorded in `ShutdownCoordinator::failures`; teardown
continues for the remaining windows and returns `ShutdownOutcome::Failed`.

## Common layout and settings contract

Grid has equal-width columns, a gap, and explicit zero-based row/column and
spans;
children left at the default (0,0) cell flow in declaration order through the
shared `grid_position` helper. GTK uses homogeneous, expanding native columns;
Tao divides the available width; AppKit splits the available container width
into equal tracks and sizes rows from natural or explicit child heights
(spanned surplus spreads evenly across the spanned rows). Flex means horizontal
wrapping with gap, not the full MAUI/CSS Flex feature set: AppKit wraps
in mount order like Tao. Absolute places children at style x/y with natural or
explicit sizes. Native font/minimum-size constraints, integer
rounding and pixel identity across backends are not abstracted away. GTK fixed
size requests remain minimums, not guaranteed maximums.

The settings slice binds editable string Draft fields directly. Parse/validation
errors do not overwrite that buffer. Saved typed config changes only after a
successful write; Cancel restores it without I/O. Dirty close asks for save,
discard or continued editing; in-flight save is awaited, and newer edits prompt
again. The app closes its Store explicitly in final View cleanup. Its settings
validation is synchronous; inherited Latest async validation remains separately
tested.

Boundaries remain explicit: legacy updates and scoped binding writes
still clone S; clone-free replacement and dependency indexing require the scoped
API.
No OS-specific input/accessibility conformance without evidence; unsupported
facilities return explicit errors, with no silent layout-kind fallback.

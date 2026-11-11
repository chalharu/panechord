//! Shared YAML native proof. Build on the host; execute only in the formal guest.
//! Native SetText is exercised; this does not claim physical keyboard/IME input.
use futures::FutureExt;
use panechord::{Runner, Session, ShutdownPhase, Store, ViewCore as _, backend};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

#[derive(Clone)]
struct Greeting {
    name: String,
}
#[panechord::commands]
impl Greeting {
    #[command]
    fn greet(&mut self) {
        self.name = format!("Hello, {}!", self.name);
    }
}

fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(format!("minimal-yaml: {message}"))
    }
}

async fn turn() {
    futures_timer::Delay::new(Duration::from_millis(400)).await;
}

async fn scenario(view: &backend::View, model: &Store<Greeting>) -> Result<(), String> {
    let mut initial = None;
    for _ in 0..20 {
        turn().await;
        if let Some(actual) = backend::smoke::native_text_probe(view, "name", "greeting", None)? {
            initial = Some(actual);
            break;
        }
    }
    let initial = initial.ok_or("minimal-yaml: native root/controls never Loaded")?;
    require(
        initial.entry_text == "Panechord" && initial.label_text == "Panechord",
        "initial native TextBox/TextBlock text mismatch",
    )?;
    const INPUT: &str = "Native shared YAML roundtrip";
    // No Store.update, View.edit or callback invocation: the real native event
    // must write the TwoWay binding generated from the unchanged shared YAML.
    backend::smoke::native_text_probe(view, "name", "greeting", Some(INPUT))?
        .ok_or("minimal-yaml: native controls unloaded before SetText")?;
    for _ in 0..20 {
        turn().await;
        let Some(actual) = backend::smoke::native_text_probe(view, "name", "greeting", None)?
        else {
            continue;
        };
        require(
            actual.generation == initial.generation
                && actual.hwnd == initial.hwnd
                && actual.entry_token == initial.entry_token
                && actual.label_token == initial.label_token,
            "native identity changed during roundtrip",
        )?;
        if actual.text_events > initial.text_events
            && model.snapshot().name == INPUT
            && actual.entry_text == INPUT
            && actual.label_text == INPUT
        {
            require(
                view.error().is_none(),
                "View reported a native/binding error",
            )?;
            eprintln!(
                "PROBE minimal-yaml NATIVE_MOUNT_EFFECT generation={} hwnd=0x{:x} Loaded=true shared-XamlRoot=true entry-token={} label-token={} text-events={} Store={:?} TextBox={:?} TextBlock={:?} TextBox.SetText->TextChanged->Store->TextBlock no-physical-keyboard-claim",
                actual.generation,
                actual.hwnd,
                actual.entry_token,
                actual.label_token,
                actual.text_events,
                model.snapshot().name,
                actual.entry_text,
                actual.label_text
            );
            return Ok(());
        }
    }
    Err("minimal-yaml: native TextChanged/Store/TextBlock roundtrip did not complete".into())
}

fn run() -> Result<(), String> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    require(
        arguments == ["--case", "minimal-yaml"],
        "usage: windows_smoke.exe --case minimal-yaml",
    )?;
    let completed = Rc::new(Cell::new(false));
    let passed = completed.clone();
    let captured = Rc::new(RefCell::new(None));
    let capture = captured.clone();
    Runner::builder()
        .initial_window_size(640, 360)
        .run(move |host| {
            let session = Session::new();
            host.attach(session.clone())?;
            let owner = Store::new(
                Greeting {
                    name: "Panechord".into(),
                },
                host.executor(),
            )
            .try_into_owner()
            .unwrap_or_else(|_| panic!("exclusive Greeting owner required"));
            let model = owner.store().clone();
            let view = panechord::owned_view!("ui/minimal.yaml", owner, backend);
            host.register_view(&session, "main", &view)?;
            let diagnostic_host = backend::smoke::view_host(&view)?;
            capture.replace(Some((
                diagnostic_host.clone(),
                session,
                view.clone(),
                model.clone(),
            )));
            let deadline = diagnostic_host.clone();
            host.executor().spawn(
                async move {
                    futures_timer::Delay::new(Duration::from_secs(20)).await;
                    deadline.fail(
                        "minimal-yaml: 20s watchdog expired before natural native return".into(),
                    );
                }
                .boxed_local(),
            );
            host.executor().spawn(
                async move {
                    match scenario(&view, &model).await {
                        Ok(()) => {
                            passed.set(true);
                            // Use normal per-window confirmation/close, not Host.close,
                            // Session Quit or Application.Exit on the success path.
                            view.request_close();
                        }
                        Err(error) => diagnostic_host.fail(error),
                    }
                }
                .boxed_local(),
            );
            Ok(())
        })?;
    // The owned driver's successful return additionally validates native
    // NCDESTROY, no native instances/bridge/timer, and natural Application.Start.
    let capture = captured.borrow();
    let (host, session, view, model) = capture
        .as_ref()
        .ok_or("minimal-yaml: Runner did not construct the YAML View")?;
    require(
        completed.get(),
        "Runner returned before native checks completed",
    )?;
    require(
        host.is_closed()
            && host.registered_views().is_empty()
            && session.window_count() == 0
            && session.shutdown().phase() == ShutdownPhase::Closed
            && view.is_closed()
            && model.is_closed()
            && host.error().is_none(),
        "Host/Session/View/owned Store teardown incomplete",
    )?;
    eprintln!(
        "PROBE minimal-yaml close-accepted host-closed=true registrations=0 session-windows=0 owned-store-closed=true natural-Start-return"
    );
    println!("NATIVE_PROBE_PASS minimal-yaml normal-exit");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("PROBE minimal-yaml FAIL {error}");
        std::process::exit(1);
    }
}

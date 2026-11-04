//! Application setup only; behavior, extension primitives and probes are separate.
//! The backend is selected by feature through the facade (`panechord::backend`).
mod model;
mod native;
mod smoke;

use panechord::backend;

use model::{RowCommands, Settings, SettingsCommands};
use native::{NumberText, meter, validate_name};
use panechord_runtime::{BlockingPool, Session, Store, UiHost, WindowHost};
use std::{cell::RefCell, rc::Rc};

fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg != "--smoke") {
        return Err(
            "usage: panechord-demo [--smoke] (minimal example: cargo run --example minimal)".into(),
        );
    }
    let workers = Rc::new(RefCell::new(BlockingPool::new(2, 8)));
    let pool = workers.clone();
    let result = backend::run_host(move |host| {
        let session = Session::new();
        host.attach(session.clone())?;
        let executor = host.executor();
        let path = std::env::temp_dir().join(format!("panechord-demo-{}.txt", std::process::id()));
        let model = Store::new(
            Settings::new(executor.clone(), pool, path.clone()),
            executor.clone(),
        );
        let owner = model
            .try_into_owner()
            .unwrap_or_else(|_| panic!("shared model"));
        let model = owner.store().clone();
        let view = panechord_macros::owned_view!("ui/gallery.yaml", owner, backend);
        host.register_view(&session, "main", &view)?;
        if args.iter().any(|arg| arg == "--smoke") {
            executor.spawn(Box::pin(smoke::verify(view.clone(), model, path)));
        }
        Ok(())
    });
    // Blocking joins never happen inside the live native loop.
    workers.borrow_mut().shutdown();
    result
}

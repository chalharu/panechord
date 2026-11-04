//! Standalone metadata browser; no nerust or persistence integration.
mod model;
mod scan;
mod smoke;
#[cfg(test)]
mod tests;
mod wiring;

use panechord::backend;

use panechord_runtime::{BlockingPool, Session, Store, UiHost, WindowHost};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
};

fn main() -> Result<(), String> {
    let (folder, smoke) = options(std::env::args_os().skip(1))?;
    let sandbox = if smoke {
        Some(smoke::Sandbox::new()?)
    } else {
        None
    };
    if let Some(sandbox) = &sandbox {
        sandbox.populate()?;
    }
    let folder = sandbox
        .as_ref()
        .map_or(folder, |sandbox| sandbox.dir.clone());
    let folder = folder
        .into_os_string()
        .into_string()
        .map_err(|_| "Folder must be representable as UTF-8 for the TextBox")?;
    let workers = Rc::new(RefCell::new(BlockingPool::new(1, 4)));
    let pool = workers.clone();
    let passed = Rc::new(Cell::new(false));
    let probe = passed.clone();
    let result = backend::run_host(move |host| {
        let session = Session::new();
        host.attach(session.clone())?;
        let executor = host.executor();
        let model = Store::new(model::App::new(folder), executor.clone());
        let view = wiring::view(model.clone(), executor.clone(), pool);
        host.register_view(&session, "main", &view)?;
        if smoke {
            executor.spawn(Box::pin(smoke::verify(view.clone(), model, probe)));
        }
        Ok(())
    });
    workers.borrow_mut().shutdown(); // Join only after the native event loop exits.
    result?;
    if smoke && !passed.get() {
        return Err("LIBRARY3 smoke did not finish".into());
    }
    drop(sandbox);
    Ok(())
}

fn options(args: impl Iterator<Item = std::ffi::OsString>) -> Result<(PathBuf, bool), String> {
    let mut args = args;
    let mut folder = None;
    let mut smoke = false;
    while let Some(arg) = args.next() {
        if arg == "--smoke" && !smoke {
            smoke = true;
        } else if arg == "--folder" && folder.is_none() {
            folder = Some(PathBuf::from(
                args.next().ok_or("--folder requires a path")?,
            ));
        } else {
            return Err("usage: panechord-library [--folder PATH] [--smoke]".into());
        }
    }
    Ok((
        folder.map_or_else(|| std::env::current_dir().map_err(|e| e.to_string()), Ok)?,
        smoke,
    ))
}

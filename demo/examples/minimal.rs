//! User-facing minimum: a model, its behavior, and a view macro. No contract crate.
//! The backend is selected by feature through the facade (`panechord::backend`).
use panechord::backend;
use panechord_runtime::Store;

#[derive(Clone)]
struct Greeting {
    name: String,
}
#[panechord_macros::commands]
impl Greeting {
    #[command]
    fn greet(&mut self) {
        self.name = format!("Hello, {}!", self.name);
    }
}
fn main() -> Result<(), String> {
    backend::run_host(|host| {
        #[cfg(not(feature = "backend-windows"))]
        use panechord_runtime::WindowHost as _;
        use panechord_runtime::{Session, UiHost as _};
        let session = Session::new();
        host.attach(session.clone())?;
        let executor = host.executor();
        let model = Store::new(
            Greeting {
                name: "Panechord".into(),
            },
            executor,
        );
        let owner = model
            .try_into_owner()
            .unwrap_or_else(|_| panic!("shared model"));
        let view = panechord_macros::owned_view!("ui/minimal.yaml", owner, backend);
        host.register_view(&session, "main", &view)?;
        Ok(())
    })
}

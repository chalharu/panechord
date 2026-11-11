//! Native smoke fixture. Execute only on the isolated test desktop.
use panechord_windows::{
    Backend,
    smoke::{self, Mode},
};
use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let Some(mode) = parse_mode(&arguments) else {
        return usage();
    };
    match smoke::run::<Backend>(mode) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("WINDOWS_ERROR {error}");
            ExitCode::FAILURE
        }
    }
}

fn parse_mode(arguments: &[String]) -> Option<Mode> {
    Some(match arguments {
        [] => Mode::Smoke,
        [argument] => match argument.as_str() {
            "--smoke" | "--p1-smoke" => Mode::Smoke,
            "--make-error" => Mode::MakeError,
            "--task-panic" => Mode::TaskPanic,
            "--reject-wake" => Mode::RejectWake,
            _ => return None,
        },
        _ => return None,
    })
}

fn usage() -> ExitCode {
    eprintln!("Usage: native_smoke [--smoke|--p1-smoke|--make-error|--task-panic|--reject-wake]");
    ExitCode::from(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_smoke_aliases_and_fault_modes() {
        assert_eq!(parse_mode(&[]), Some(Mode::Smoke));
        for (argument, mode) in [
            ("--smoke", Mode::Smoke),
            ("--p1-smoke", Mode::Smoke),
            ("--make-error", Mode::MakeError),
            ("--task-panic", Mode::TaskPanic),
            ("--reject-wake", Mode::RejectWake),
        ] {
            assert_eq!(parse_mode(&[argument.into()]), Some(mode));
        }
        assert_eq!(parse_mode(&["--unknown".into()]), None);
        assert_eq!(parse_mode(&["--smoke".into(), "--make-error".into()]), None);
    }
}

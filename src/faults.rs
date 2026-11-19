//! Test-only, thread-local failure injection. Absent from normal library/CLI builds.
use std::cell::Cell;

use crate::Error;
use crate::error::Position;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fault {
    Allocation(&'static str),
    OutputBytes,
    OutputTokens,
    FinalCounts,
}

thread_local! {
    static FAULT: Cell<Option<Fault>> = const { Cell::new(None) };
}

pub(crate) fn with<T>(fault: Fault, action: impl FnOnce() -> T) -> T {
    struct Restore(Option<Fault>);
    impl Drop for Restore {
        fn drop(&mut self) {
            FAULT.set(self.0);
        }
    }
    let _restore = Restore(FAULT.replace(Some(fault)));
    action()
}

pub(crate) fn take(fault: Fault) -> bool {
    if FAULT.get() == Some(fault) {
        FAULT.set(None);
        true
    } else {
        false
    }
}

pub(crate) fn allocation(area: &str, position: Position) -> Result<(), Error> {
    let fail = matches!(FAULT.get(), Some(Fault::Allocation(expected)) if expected == area);
    if fail {
        FAULT.set(None);
        Err(Error::at(
            position,
            format!("unable to allocate {area}: injected test failure"),
        ))
    } else {
        Ok(())
    }
}

//! What a session owns beyond its variables: its host, random numbers, units, currency rates, the network switch, the
//! stopwatch. Each `Interp` has its own, and code deep in units or rates reaches the running session's through `with`.

use crate::error::Limit;
use crate::host::{self, Host};
use crate::modules::units::{UnitDef, builtin_units};
use jiff::Timestamp;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

pub struct State {
    pub host: Rc<dyn Host>,
    pub rng: RefCell<fastrand::Rng>,
    pub units: RefCell<HashMap<String, Rc<UnitDef>>>,
    /// Units made with `unit`: name and what it was defined as, for `help`.
    pub user_units: RefCell<Vec<(String, String)>>,
    pub rates_loaded: Cell<bool>,
    pub rates_error: RefCell<String>,
    /// Off in every new session; `allow_network_access()` turns it on for good.
    pub network: Cell<bool>,
    pub stopwatch: Cell<Option<Timestamp>>,
    /// The directory before the last `cd`, for `cd -`.
    pub prev_dir: RefCell<Option<String>>,
    /// Set while completion evaluates a receiver: no writes outside it, no `mutates` or `shown` fns, no rate fetching.
    pub probing: Cell<bool>,
    /// `Interp::limits`' size caps, copied in by `run` and `call` so builtins and operators can read them.
    pub max_len: Cell<usize>,
    pub max_bits: Cell<u64>,
    pub max_output: Cell<usize>,
    /// Bytes printed since the last `run` began.
    pub printed: Cell<usize>,
    /// The size or output cap the last failed check hit, for `run` and `call` to put on their error.
    // ponytail: a check whose error a builtin swallows leaves this set, so a later unrelated error in the run would carry it; pass it inside the error if that happens
    pub hit: Cell<Option<Limit>>,
}

impl State {
    pub fn new(host: Rc<dyn Host>) -> Rc<State> {
        Rc::new(State {
            rng: RefCell::new(fastrand::Rng::with_seed(host.seed())),
            host,
            units: RefCell::new(builtin_units()),
            user_units: Default::default(),
            rates_loaded: Cell::new(false),
            rates_error: Default::default(),
            network: Cell::new(false),
            stopwatch: Cell::new(None),
            prev_dir: Default::default(),
            probing: Cell::new(false),
            max_len: Cell::new(usize::MAX),
            max_bits: Cell::new(u64::MAX),
            max_output: Cell::new(usize::MAX),
            printed: Cell::new(0),
            hit: Cell::new(None),
        })
    }
}

thread_local! {
    /// The session running now; outside any, a throwaway one.
    static CURRENT: RefCell<Rc<State>> = RefCell::new(State::new(host::default()));
}

/// Gives the code outside any session, and so every `Interp::new` after, this `host`.
pub fn set_default_host(host: Rc<dyn Host>) {
    CURRENT.set(State::new(host));
}

/// The running session's state.
pub fn with<T>(f: impl FnOnce(&State) -> T) -> T {
    f(&CURRENT.with_borrow(Rc::clone))
}

/// Errors unless `n` items (of `what`) fit the running session's size cap.
pub fn check_len(n: usize, what: &str) -> Result<(), String> {
    let max = with(|s| s.max_len.get());
    if n > max {
        with(|s| s.hit.set(Some(Limit::Len)));
        Err(format!(
            "result too large
note: the limit is {max} {what}; this one has {n}"
        ))
    } else {
        Ok(())
    }
}

/// Errors unless an exact number of `bits` bits fits the running session's size cap.
pub fn check_bits(bits: u64) -> Result<(), String> {
    let max = with(|s| s.max_bits.get());
    if bits > max {
        with(|s| s.hit.set(Some(Limit::Bits)));
        Err(format!(
            "result too large
note: exact results stop at {max} bits, about {} digits",
            (max as f64 * std::f64::consts::LOG10_2) as u64
        ))
    } else {
        Ok(())
    }
}

/// Counts `n` more bytes of output, erroring once the run's total passes the session's output cap.
pub fn check_output(n: usize) -> Result<(), String> {
    with(|s| {
        let total = s.printed.get().saturating_add(n);
        s.printed.set(total);
        let max = s.max_output.get();
        if total > max {
            s.hit.set(Some(Limit::Output));
            Err(format!(
                "output limit reached
note: the limit is {max} bytes per run"
            ))
        } else {
            Ok(())
        }
    })
}

/// Runs `f` with `state` as the running session's, then puts back the one before.
pub fn enter<T>(state: &Rc<State>, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<Rc<State>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CURRENT.set(self.0.take().unwrap());
        }
    }
    let _restore = Restore(Some(CURRENT.replace(state.clone())));
    f()
}

#[cfg(test)]
mod tests {
    use crate::{Interp, run};

    #[test]
    fn sessions_keep_their_own_state() {
        let (mut a, mut b) = (Interp::new(), Interp::new());
        run(&mut a, "unit pizza; allow_network_access()").unwrap();
        assert_eq!(run(&mut a, "2 pizza").unwrap().to_string(), "2 pizza");
        assert!(run(&mut b, "2 pizza").is_err());
        assert!(a.state().network.get() && !b.state().network.get());
        assert!(!super::with(|s| s.network.get()), "no session leaks into the code outside one");
    }
}

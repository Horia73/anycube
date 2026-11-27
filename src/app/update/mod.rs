//! The native launcher receives a private capability in the loopback URL fragment.
#[cfg(any(target_os = "macos", all(unix, test)))]
mod package;
#[cfg(target_os = "macos")]
mod mac;

use serde_json::{json, Value};
use std::sync::{Mutex, RwLock};

#[derive(Clone)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
struct Offer {
    id: String,
    version: String,
    notes: String,
    asset: String,
    digest: String,
    bytes: u64,
    build: u64,
}

#[derive(Default)]
struct State {
    offer: Option<Offer>,
    generation: u64,
    phase: &'static str,
    bytes: u64,
    message: String,
    restart_at: Option<std::time::Instant>,
}

impl State {
    fn acceptance(&self) -> Option<String> {
        self.offer.as_ref().map(|offer| format!("{}-{}", offer.id, self.generation))
    }
}

pub(super) struct Update {
    // Only a launcher that requested a native session receives this secret.
    token: String,
    state: Mutex<State>,
    // Robot and solve mutations take a read lock; install takes the write lock.
    pub work: RwLock<()>,
    #[cfg(target_os = "macos")]
    bundle: std::path::PathBuf,
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
impl Update {
    pub fn desktop() -> Option<Self> { None }
}

impl Update {
    pub fn launch_url(&self, url: &str) -> String {
        format!("{url}?window#update={}", self.token)
    }

    pub fn authorized(&self, headers: &[tiny_http::Header], origin: &str) -> bool {
        let header = |name: &'static str| {
            let mut values = headers.iter().filter(|h| h.field.equiv(name));
            let value = values.next()?.value.as_str();
            values.next().is_none().then_some(value)
        };
        let token_ok = header("X-Anycube-Desktop").is_some_and(|token| {
            token.len() == self.token.len() &&
                token.bytes().zip(self.token.bytes()).fold(0, |different, (a, b)| different | (a ^ b)) == 0
        });
        header("Origin") == Some(origin) && header("Content-Type") == Some("application/json") && token_ok
    }

    pub fn restarting(&self) -> bool {
        self.state.lock().unwrap().phase == "restarting"
    }

    pub fn ready_to_exit(&self) -> bool {
        self.state.lock().unwrap().restart_at.is_some_and(|time| time.elapsed() >= std::time::Duration::from_millis(750))
    }

    pub fn installing(&self) -> bool {
        matches!(self.state.lock().unwrap().phase, "installing" | "restarting")
    }

    #[cfg(any(target_os = "macos", all(unix, test)))]
    fn activate_staged(
        &self, executor: &impl package::Executor, installed: &std::path::Path,
        staged: &std::path::Path, pending_swap: &std::path::Path, busy: impl Fn() -> bool,
    ) -> Result<(), (String, bool)> {
        let _gate = self.work.write().unwrap();
        {
            let mut state = self.state.lock().unwrap();
            if state.phase == "cancelling" { return Err(("Cancelled".into(), false)); }
            if busy() { return Err(("Finish your work and disconnect the robot before installing.".into(), false)); }
            state.phase = "installing";
        }
        std::fs::File::create(pending_swap).and_then(|file| file.sync_all())
            .map_err(|error| (error.to_string(), false))?;
        package::activate(executor, installed, staged)?;
        // Block any new work as soon as the replacement has been launched.
        self.state.lock().unwrap().phase = "restarting";
        Ok(())
    }

    fn view(state: &State) -> Value {
        json!({
            "phase": state.phase, "bytes": state.bytes, "message": state.message,
            "current": env!("CARGO_PKG_VERSION"),
            "current_build": option_env!("ANYCUBE_UPDATE_BUILD").and_then(|value| value.parse::<u64>().ok()),
            "offer": state.offer.as_ref().map(|offer| json!({
                "id": state.acceptance(), "version": offer.version, "notes": offer.notes, "bytes": offer.bytes,
                "build": offer.build,
            })),
        })
    }

    pub fn request(&self, action: &str, body: &Value, busy: impl Fn() -> bool) -> (u16, Value) {
        self.request_with(action, body, &busy, platform_check, |offer| self.install(offer, &busy))
    }

    fn request_with(
        &self,
        action: &str,
        body: &Value,
        busy: impl Fn() -> bool,
        check: impl FnOnce() -> Result<Option<Offer>, String>,
        install: impl FnOnce(&Offer) -> Result<(), String>,
    ) -> (u16, Value) {
        let mut state = self.state.lock().unwrap();
        match action {
            "status" => {}
            "later" if state.phase == "downloading" || state.phase == "verifying" => {
                state.phase = "cancelling";
            }
            "later" if state.phase == "checking" => {
                state.phase = "dismissing";
            }
            "later" if !Self::active(&state) && state.phase != "restarting" => {
                state.offer = None;
                state.phase = "later";
            }
            "check" if !Self::active(&state) && state.phase != "restarting" => {
                let generation = state.generation + 1;
                *state = State { phase: "checking", generation, ..State::default() };
                drop(state);
                let result = check();
                state = self.state.lock().unwrap();
                if state.phase == "dismissing" {
                    state.phase = "later";
                    return (200, Self::view(&state));
                }
                match result {
                    Ok(offer) => {
                        state.phase = if offer.is_some() { "available" } else { "current" };
                        state.offer = offer;
                    }
                    Err(error) => Self::failed(&mut state, error),
                }
            }
            "install" if matches!(state.phase, "available" | "error" | "cancelled") => {
                let Some(acceptance) = state.acceptance().filter(|id| body == &json!({"id": id})) else {
                    return (409, json!({"message": "Review the current offer before installing."}));
                };
                // Lock order is work, then state. Recheck the one-use acceptance after taking the gate.
                drop(state);
                let gate = self.work.write().unwrap();
                state = self.state.lock().unwrap();
                if !matches!(state.phase, "available" | "error" | "cancelled")
                    || state.acceptance().as_ref() != Some(&acceptance) {
                    return (409, Self::view(&state));
                }
                let offer = state.offer.clone().expect("acceptance requires an offer");
                state.generation += 1;
                if busy() {
                    Self::failed(&mut state, "Finish your work and disconnect the robot before installing.".into());
                    return (409, Self::view(&state));
                }
                state.phase = "downloading";
                state.message.clear();
                state.bytes = 0;
                drop(state);
                drop(gate);
                let result = install(&offer);
                state = self.state.lock().unwrap();
                match result {
                    Ok(()) => {
                        state.phase = "restarting";
                        state.restart_at = Some(std::time::Instant::now());
                    }
                    Err(_) if state.phase == "cancelling" => {
                        state.phase = "cancelled";
                        state.message = "Download cancelled.".into();
                    }
                    Err(error) => Self::failed(&mut state, error),
                }
            }
            _ => return (409, Self::view(&state)),
        }
        (200, Self::view(&state))
    }

    fn active(state: &State) -> bool {
        matches!(state.phase, "checking" | "dismissing" | "downloading" | "verifying" | "cancelling" | "installing")
    }

    fn failed(state: &mut State, error: String) {
        state.phase = "error";
        state.message = error;
    }
}

#[cfg(target_os = "macos")]
use mac::platform_check;

#[cfg(not(target_os = "macos"))]
fn platform_check() -> Result<Option<Offer>, String> { Ok(None) }

#[cfg(not(target_os = "macos"))]
impl Update {
    fn install(&self, _: &Offer, _: impl Fn() -> bool) -> Result<(), String> {
        Err("Desktop updates require the packaged macOS app.".into())
    }
}

#[cfg(all(test, unix))]
mod tests;

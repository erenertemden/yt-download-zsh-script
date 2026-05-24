use std::{
    process::{Child, Command, ExitStatus},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

pub type SharedChild = Arc<Mutex<Child>>;

#[derive(Clone, Default)]
pub struct ProcessControl {
    cancelled: Arc<AtomicBool>,
    active_child: Arc<Mutex<Option<SharedChild>>>,
}

impl ProcessControl {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) -> bool {
        let was_cancelled = self.cancelled.swap(true, Ordering::SeqCst);
        self.kill_active_child();
        !was_cancelled
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn set_child(&self, child: SharedChild) {
        if let Ok(mut active_child) = self.active_child.lock() {
            *active_child = Some(child.clone());
        }

        if self.is_cancelled() {
            kill_child(&child);
        }
    }

    pub fn clear_child(&self) {
        if let Ok(mut active_child) = self.active_child.lock() {
            *active_child = None;
        }
    }

    fn kill_active_child(&self) {
        let child = self
            .active_child
            .lock()
            .ok()
            .and_then(|active_child| active_child.clone());

        if let Some(child) = child {
            kill_child(&child);
        }
    }
}

pub fn shared_child(child: Child) -> SharedChild {
    Arc::new(Mutex::new(child))
}

#[cfg(unix)]
pub fn prepare_command(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
}

#[cfg(not(unix))]
pub fn prepare_command(_command: &mut Command) {}

pub fn wait_for_child(
    child: &SharedChild,
    control: &ProcessControl,
    process_name: &str,
) -> Result<ExitStatus, String> {
    loop {
        if control.is_cancelled() {
            kill_child(child);
        }

        let status = {
            let mut child = child
                .lock()
                .map_err(|_| format!("{process_name} process lock was poisoned."))?;
            child
                .try_wait()
                .map_err(|error| format!("{process_name} failed to report status: {error}"))?
        };

        if let Some(status) = status {
            return Ok(status);
        }

        thread::sleep(Duration::from_millis(100));
    }
}

fn kill_child(child: &SharedChild) {
    if let Ok(mut child) = child.lock() {
        #[cfg(unix)]
        signal_process_group(child.id(), unix_signal::SIGTERM);
        let _ = child.kill();
        #[cfg(unix)]
        signal_process_group(child.id(), unix_signal::SIGKILL);
    }
}

#[cfg(unix)]
fn signal_process_group(pid: u32, signal: i32) {
    let Ok(pid) = i32::try_from(pid) else {
        return;
    };

    unix_signal::kill_process_group(pid, signal);
}

#[cfg(unix)]
mod unix_signal {
    pub const SIGTERM: i32 = 15;
    pub const SIGKILL: i32 = 9;

    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }

    pub fn kill_process_group(pid: i32, signal: i32) {
        unsafe {
            let _ = kill(-pid, signal);
        }
    }
}

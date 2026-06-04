use crate::usage::Service;
use std::{
    collections::HashMap,
    process::Child,
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

pub const PROFILE_STOP_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug)]
pub struct BrowserSessionManager {
    processes: Mutex<HashMap<Service, ManagedBrowserProcess>>,
}

#[derive(Debug)]
struct ManagedBrowserProcess {
    process_id: u32,
    child: Child,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserSessionStopStatus {
    NoManagedProcess,
    AlreadyExited,
    Stopped,
    Killed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrowserSessionStopResult {
    pub service: Service,
    pub status: BrowserSessionStopStatus,
}

impl Default for BrowserSessionManager {
    fn default() -> Self {
        Self {
            processes: Mutex::new(HashMap::new()),
        }
    }
}

impl BrowserSessionManager {
    #[allow(dead_code)]
    pub fn track_process(&self, service: Service, mut child: Child) -> Result<u32, String> {
        let mut processes = self
            .processes
            .lock()
            .map_err(|_| "Browser session state is unavailable".to_string())?;

        if processes.contains_key(&service) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Managed browser process already exists".to_string());
        }

        let process_id = child.id();
        processes.insert(service, ManagedBrowserProcess { process_id, child });
        Ok(process_id)
    }

    pub fn stop_service(
        &self,
        service: Service,
        timeout: Duration,
    ) -> Result<BrowserSessionStopResult, String> {
        let mut processes = self
            .processes
            .lock()
            .map_err(|_| "Browser session state is unavailable".to_string())?;

        if !processes.contains_key(&service) {
            return Ok(BrowserSessionStopResult {
                service,
                status: BrowserSessionStopStatus::NoManagedProcess,
            });
        }

        let status = {
            let process = processes
                .get_mut(&service)
                .ok_or_else(|| "Browser session state is unavailable".to_string())?;
            stop_process(process, timeout)?
        };

        processes.remove(&service);
        Ok(BrowserSessionStopResult { service, status })
    }
}

fn stop_process(
    process: &mut ManagedBrowserProcess,
    timeout: Duration,
) -> Result<BrowserSessionStopStatus, String> {
    if process
        .child
        .try_wait()
        .map_err(|_| "Could not inspect managed browser process".to_string())?
        .is_some()
    {
        return Ok(BrowserSessionStopStatus::AlreadyExited);
    }

    request_graceful_shutdown(process)?;
    if wait_for_exit(&mut process.child, timeout)? {
        return Ok(BrowserSessionStopStatus::Stopped);
    }

    process
        .child
        .kill()
        .map_err(|_| "Could not stop managed browser process".to_string())?;
    process
        .child
        .wait()
        .map_err(|_| "Could not reap managed browser process".to_string())?;
    Ok(BrowserSessionStopStatus::Killed)
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> Result<bool, String> {
    let deadline = Instant::now() + timeout;

    loop {
        if child
            .try_wait()
            .map_err(|_| "Could not inspect managed browser process".to_string())?
            .is_some()
        {
            return Ok(true);
        }

        if Instant::now() >= deadline {
            return Ok(false);
        }

        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn request_graceful_shutdown(process: &ManagedBrowserProcess) -> Result<(), String> {
    let result = unsafe { libc::kill(process.process_id as libc::pid_t, libc::SIGTERM) };

    if result == 0 {
        return Ok(());
    }

    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }

    Err("Could not request managed browser shutdown".to_string())
}

#[cfg(not(unix))]
fn request_graceful_shutdown(_process: &ManagedBrowserProcess) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{process::Command, thread};

    #[test]
    fn stop_service_without_tracked_process_is_noop() {
        let manager = BrowserSessionManager::default();

        let result = manager
            .stop_service(Service::Codex, Duration::from_millis(1))
            .expect("stop succeeds");

        assert_eq!(
            result,
            BrowserSessionStopResult {
                service: Service::Codex,
                status: BrowserSessionStopStatus::NoManagedProcess,
            }
        );
    }

    #[test]
    fn track_process_refuses_duplicate_service_owner() {
        let manager = BrowserSessionManager::default();
        let first = sleeping_child();
        let second = sleeping_child();

        manager
            .track_process(Service::Codex, first)
            .expect("first process is tracked");
        let error = manager
            .track_process(Service::Codex, second)
            .expect_err("duplicate process is rejected");

        assert_eq!(error, "Managed browser process already exists");
        let result = manager
            .stop_service(Service::Codex, Duration::from_secs(1))
            .expect("tracked process stops");
        assert_ne!(result.status, BrowserSessionStopStatus::NoManagedProcess);
    }

    #[test]
    fn stop_service_reaps_exited_process() {
        let manager = BrowserSessionManager::default();
        let child = exited_child();

        manager
            .track_process(Service::Claude, child)
            .expect("process is tracked");
        thread::sleep(Duration::from_millis(50));
        let result = manager
            .stop_service(Service::Claude, Duration::from_millis(1))
            .expect("tracked process stops");

        assert_eq!(result.status, BrowserSessionStopStatus::AlreadyExited);
    }

    #[test]
    fn stop_service_terminates_running_process() {
        let manager = BrowserSessionManager::default();
        let child = sleeping_child();

        manager
            .track_process(Service::Codex, child)
            .expect("process is tracked");
        let result = manager
            .stop_service(Service::Codex, Duration::from_secs(1))
            .expect("tracked process stops");

        assert!(matches!(
            result.status,
            BrowserSessionStopStatus::Stopped | BrowserSessionStopStatus::Killed
        ));
    }

    #[cfg(unix)]
    fn sleeping_child() -> Child {
        Command::new("sh")
            .arg("-c")
            .arg("sleep 30")
            .spawn()
            .expect("sleep process starts")
    }

    #[cfg(not(unix))]
    fn sleeping_child() -> Child {
        Command::new("cmd")
            .args(["/C", "ping -n 30 127.0.0.1 >NUL"])
            .spawn()
            .expect("sleep process starts")
    }

    #[cfg(unix)]
    fn exited_child() -> Child {
        Command::new("sh")
            .arg("-c")
            .arg("exit 0")
            .spawn()
            .expect("short process starts")
    }

    #[cfg(not(unix))]
    fn exited_child() -> Child {
        Command::new("cmd")
            .args(["/C", "exit 0"])
            .spawn()
            .expect("short process starts")
    }
}

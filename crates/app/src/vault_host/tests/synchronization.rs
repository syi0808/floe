use super::*;
use std::os::fd::AsRawFd;

#[derive(Default)]
pub(super) struct TestSignal {
    state: Mutex<bool>,
    ready: Condvar,
}

impl TestSignal {
    pub(super) fn set(&self, state: bool) {
        *self.state.lock().unwrap() = state;
        self.ready.notify_all();
    }

    pub(super) fn wait_until(&self, deadline: Instant) {
        let (state, _) = self
            .ready
            .wait_timeout_while(
                self.state.lock().unwrap(),
                deadline.saturating_duration_since(Instant::now()),
                |state| !*state,
            )
            .unwrap();
        assert!(*state, "test signal did not arrive before its deadline");
    }
}

pub(super) fn wait_for_job(worker: &Worker, request_id: Uuid, deadline: Instant) {
    let job = worker
        .jobs
        .lock()
        .unwrap()
        .get(&request_id)
        .unwrap()
        .clone();
    let (progress, _) = job
        .finished
        .wait_timeout_while(
            job.progress.lock().unwrap(),
            deadline.saturating_duration_since(Instant::now()),
            |progress| !progress.done,
        )
        .unwrap();
    assert!(
        progress.done,
        "Vault job did not finish before its deadline"
    );
}

pub(super) fn accept_before(listener: &TcpListener, deadline: Instant) -> TcpStream {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "mock server accept deadline exceeded");
        let mut descriptor = libc::pollfd {
            fd: listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let timeout = remaining.as_millis().clamp(1, i32::MAX as u128) as i32;
        let ready = unsafe { libc::poll(&mut descriptor, 1, timeout) };
        if ready < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            panic!("poll mock listener: {error}");
        }
        if ready == 0 {
            continue;
        }
        assert_ne!(descriptor.revents & libc::POLLIN, 0);
        match listener.accept() {
            Ok((socket, _)) => return socket,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(error) => panic!("accept: {error}"),
        }
    }
}

#[test]
fn signal_before_wait_is_not_lost() {
    let signal = TestSignal::default();
    signal.set(true);
    signal.wait_until(Instant::now());
}

#[test]
#[should_panic(expected = "test signal did not arrive before its deadline")]
fn expired_signal_wait_remains_bounded() {
    TestSignal::default().wait_until(Instant::now());
}

#[test]
fn listener_readiness_accepts_a_queued_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let socket = accept_before(&listener, Instant::now() + Duration::from_secs(5));
    assert_eq!(socket.peer_addr().unwrap(), client.local_addr().unwrap());
}

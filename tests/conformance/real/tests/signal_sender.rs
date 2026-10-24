//! Sender attribution through the shell and io on the Linux kernel.
#![cfg(target_os = "linux")]

fn service(sender: &str) {
    for backend in ["ring", "readiness"] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_signal_sender_fixture"))
            .args([backend, sender])
            .output()
            .expect("the single-threaded signal fixture starts");
        assert!(
            output.status.success(),
            "{backend} signal from {sender}: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn a_spawned_childs_sigterm_names_its_io_token() {
    service("child");
}

#[test]
fn a_self_sigterm_has_no_child_token() {
    service("self");
}

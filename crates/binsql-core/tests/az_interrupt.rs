//! A Key Vault read that runs out of time must not leave `az` behind. This is
//! the only test here because it puts a stub `az` on the process's `PATH`.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

use binsql_core::secrets::{Cache, Resolver};

/// Whether `pid` still runs. A killed child stays a zombie until tokio reaps
/// it, which needs the runtime driven, so a zombie counts as gone.
fn alive(pid: &str) -> bool {
    std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", pid])
        .output()
        .is_ok_and(|ps| {
            let stat = String::from_utf8_lossy(&ps.stdout);
            let stat = stat.trim();
            !stat.is_empty() && !stat.starts_with('Z')
        })
}

#[test]
fn dropping_a_vault_read_kills_az() {
    let dir = std::env::temp_dir().join(format!("binsql-az-interrupt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    let pidfile = dir.join("az.pid");

    let stub = dir.join("az");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\necho $$ > '{}'\nexec sleep 30\n",
            pidfile.display()
        ),
    )
    .expect("write the stub");
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755))
        .expect("make the stub runnable");

    let path = std::env::var("PATH").unwrap_or_default();
    // SAFETY: this is the binary's only test, and no other thread exists yet.
    unsafe { std::env::set_var("PATH", format!("{}:{path}", dir.display())) };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build a runtime");
    let resolver = Resolver::new(Cache::new(&dir, Duration::ZERO), None);
    let read = runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_millis(500),
            resolver.resolve_fresh("keyvault://kv-demo/app-dsn"),
        )
        .await
    });
    assert!(read.is_err(), "the stub sleeps, so the read must time out");

    let pid = std::fs::read_to_string(&pidfile).expect("the stub ran");
    let pid = pid.trim();
    let started = Instant::now();
    while alive(pid) && started.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!alive(pid), "az ({pid}) outlived the dropped read");

    drop(runtime);
    let _ = std::fs::remove_dir_all(&dir);
}

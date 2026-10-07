//! A Key Vault read hands back what `az` printed, and one that runs out of time
//! must not leave `az` behind. This is the only test here because it puts a
//! stub `az` on the process's `PATH`.
#![cfg(unix)]

use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
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
    // A directory of its own, never one already there: the test runs what is
    // in it.
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&dir)
        .expect("create scratch dir");
    let pidfile = dir.join("az.pid");

    let stub = dir.join("az");
    std::fs::write(
        &stub,
        format!(
            // Two secrets answer at once. For any other, like the real `az`,
            // the stub starts the process doing the work without `exec`, so
            // the sleep is a grandchild of the test. The pid is written whole,
            // so a reader never sees half of it.
            "#!/bin/sh\n\
             case \"$*\" in\n\
             *' ready '*) echo '{{\"value\": \"sqlite://ready.db\"}}'; exit 0 ;;\n\
             *' denied '*) echo 'Please run az login to setup account.' >&2; exit 1 ;;\n\
             esac\n\
             sleep 30 &\necho $! > '{pid}.part'\nmv '{pid}.part' '{pid}'\nwait\n",
            pid = pidfile.display()
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

    let ready = runtime.block_on(resolver.resolve_fresh("keyvault://kv-demo/ready"));
    assert_eq!(ready.expect("the stub answers"), "sqlite://ready.db");
    let denied = runtime
        .block_on(resolver.resolve_fresh("keyvault://kv-demo/denied"))
        .expect_err("the stub refuses");
    assert!(denied.to_string().contains("az login"), "{denied}");

    // The read is driven until the stub has recorded its sleep, then dropped.
    let pid = runtime.block_on(async {
        let read = resolver.resolve_fresh("keyvault://kv-demo/app-dsn");
        tokio::pin!(read);
        let started = Instant::now();
        loop {
            tokio::select! {
                _ = &mut read => panic!("the stub sleeps, so the read cannot finish"),
                () = tokio::time::sleep(Duration::from_millis(20)) => {}
            }
            if let Ok(pid) = std::fs::read_to_string(&pidfile) {
                break pid;
            }
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "the stub never ran"
            );
        }
    });
    let pid = pid.trim();
    let started = Instant::now();
    while alive(pid) && started.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !alive(pid),
        "what az started ({pid}) outlived the dropped read"
    );

    drop(runtime);
    let _ = std::fs::remove_dir_all(&dir);
}

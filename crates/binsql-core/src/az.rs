//! Running the Azure CLI, which reads Key Vault secrets and hands the SQL
//! Server adapter its `fedauth=` tokens.

use std::ffi::OsStr;
use std::process::{Output, Stdio};

use tokio::io::{AsyncRead, AsyncReadExt};

/// Runs `az` with `args` and waits for it to finish.
///
/// Dropping the future kills `az` and everything it started. On unix `az` is
/// usually a shell script that runs Python without `exec`, so killing only the
/// process tokio spawned would leave the Python doing the work running. `az`
/// gets a process group of its own, and the group is what gets killed.
pub(crate) async fn output<I, S>(args: I) -> std::io::Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = tokio::process::Command::new("az");
    command
        .args(args)
        // The Azure CLI's Python warnings go to stderr and have broken output
        // parsing before; silencing them is the documented workaround.
        .env("PYTHONWARNINGS", "ignore")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);

    let mut child = command.spawn()?;
    // Declared after `child`, so a dropped read kills the group before tokio
    // kills and reaps its leader.
    #[cfg(unix)]
    let group = Group(child.id());
    let (stdout, stderr) =
        tokio::try_join!(drain(child.stdout.take()), drain(child.stderr.take()))?;
    // Only `wait` reaps the leader, and nothing awaits between it and `disarm`.
    let status = child.wait().await;
    #[cfg(unix)]
    group.disarm();
    Ok(Output {
        status: status?,
        stdout,
        stderr,
    })
}

async fn drain(pipe: Option<impl AsyncRead + Unpin>) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    if let Some(mut pipe) = pipe {
        pipe.read_to_end(&mut bytes).await?;
    }
    Ok(bytes)
}

/// Kills the process group led by the pid it holds when dropped.
#[cfg(unix)]
struct Group(Option<u32>);

#[cfg(unix)]
impl Group {
    /// `az` has exited and been reaped, so its id is free to be reused.
    fn disarm(mut self) {
        self.0 = None;
    }
}

#[cfg(unix)]
impl Drop for Group {
    fn drop(&mut self) {
        if let Some(Ok(leader)) = self.0.map(i32::try_from) {
            // SAFETY: `kill` takes no pointers. The group is `az`'s own, and
            // its leader has not been reaped, so the id cannot name another.
            unsafe { libc::kill(-leader, libc::SIGKILL) };
        }
    }
}

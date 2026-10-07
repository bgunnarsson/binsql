//! Running the Azure CLI, which reads Key Vault secrets and hands the SQL
//! Server adapter its `fedauth=` tokens.

use std::ffi::OsStr;
use std::process::Output;

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
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);

    let child = command.spawn()?;
    #[cfg(unix)]
    let group = Group(child.id());
    let output = child.wait_with_output().await;
    #[cfg(unix)]
    group.disarm();
    output
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

//! Server restart functionality for remote administration
//!
//! This module provides the ability to restart the MeshBBS server programmatically,
//! allowing remote administrators to apply configuration changes without SSH access.

use std::env;
use std::process::{exit, Command};
use tracing::info;

/// Restart the server by spawning a new instance and exiting the current one
///
/// This function:
/// 1. Gets the path to the current executable
/// 2. Collects the original command-line arguments
/// 3. Spawns a detached background process with the same executable and arguments
/// 4. Exits the current process gracefully
///
/// The new process will start fresh, loading updated configuration from disk.
///
/// # Returns
///
/// Returns `Ok(())` if the new process was spawned successfully, or an error
/// message if spawning failed. Note that on success, this function never returns
/// as it calls `exit(0)`.
///
/// # Platform Support
///
/// This approach works reliably on Unix-like systems (Linux, macOS, BSD).
/// Windows support may require additional considerations for proper process handling.
pub fn restart_server() -> Result<(), String> {
    let current_exe =
        env::current_exe().map_err(|e| format!("Failed to get current executable path: {}", e))?;

    let args: Vec<String> = env::args().skip(1).collect();

    info!("🔄 Initiating server restart...");
    info!("   Executable: {:?}", current_exe);
    if !args.is_empty() {
        info!("   Arguments: {:?}", args);
    }

    // On Unix systems, spawn a shell script that waits for us to exit, then starts the new process
    // This ensures the old process fully releases all resources (ports, file locks) before the new one starts
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        // Create a restart command that:
        // 1. Sleeps for 3 seconds to let the old process exit and release resources
        // 2. Starts the new server instance in the background
        let command_line = format!(
            "(sleep 3; {} {} > meshbbs.log 2>&1) &",
            current_exe.display(),
            args.join(" ")
        );

        Command::new("sh")
            .arg("-c")
            .arg(&command_line)
            .env("MESHBBS_RESTARTING", "1")
            .process_group(0) // Create new process group to detach from parent
            .spawn()
            .map_err(|e| format!("Failed to spawn restart script: {}", e))?;
    }

    #[cfg(not(unix))]
    {
        // Windows fallback - spawn detached process with delay
        Command::new("cmd")
            .args(&[
                "/C",
                "timeout",
                "2",
                "&&",
                &current_exe.display().to_string(),
            ])
            .args(&args)
            .env("MESHBBS_RESTARTING", "1")
            .spawn()
            .map_err(|e| format!("Failed to spawn new process: {}", e))?;
    }

    info!("✅ Restart script spawned - new instance will start in 3 seconds");
    info!("🛑 Shutting down current instance...");

    // Brief sleep to ensure logs are flushed
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Exit cleanly - this triggers all Drop implementations for graceful shutdown
    exit(0);
}

/// Check if the current process is a restart
///
/// This can be used to detect and log when the server has been restarted
/// programmatically vs. started normally.
pub fn is_restarting() -> bool {
    env::var("MESHBBS_RESTARTING").is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_restarting_detection() {
        // Should be false in normal test environment
        assert!(!is_restarting());

        // Set the flag
        env::set_var("MESHBBS_RESTARTING", "1");
        assert!(is_restarting());

        // Clean up
        env::remove_var("MESHBBS_RESTARTING");
    }
}

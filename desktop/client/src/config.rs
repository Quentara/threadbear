// Where the runner is: its data dir (for the token) and its port, found
// the way runner/src/config.ts and `tb` find them, so pointing
// THREADBEAR_HOME at a dev runner or the mock works the same here.
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const DEFAULT_PORT: u16 = 8797;

#[derive(Clone, Debug)]
pub struct RunnerConfig {
    pub home: PathBuf,
    pub port: u16,
}

impl RunnerConfig {
    pub fn from_env() -> Self {
        let home = std::env::var("THREADBEAR_HOME")
            .or_else(|_| std::env::var("THREADBEAR_DATA_DIR"))
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| expand_home(&s))
            .unwrap_or_else(default_home);
        let port = std::env::var("THREADBEAR_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .or_else(|| port_from_runner_json(&home))
            .unwrap_or(DEFAULT_PORT);
        RunnerConfig { home, port }
    }

    pub fn token_path(&self) -> PathBuf {
        self.home.join("secrets").join("runner-token")
    }

    pub fn url(&self) -> String {
        format!("ws://127.0.0.1:{}/rpc", self.port)
    }

    /// Read fresh on every connect: the runner makes a new one when it's
    /// reinstalled, and the mock writes its own.
    pub fn read_token(&self) -> Result<String, String> {
        std::fs::read_to_string(self.token_path())
            .map(|s| s.trim().to_string())
            .map_err(|e| format!("Couldn't read the runner's token at {}: {e}", self.token_path().display()))
    }
}

/// The runner keeps the Mac's folder names on every platform for now
/// (INSTALL.md, "A runner on Linux"). A Windows runner may choose
/// %APPDATA%\Threadbear instead; this follows whatever it picks.
pub fn default_home() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join("Library").join("Application Support").join("Threadbear")
}

/// Why a scripted app (one with THREADBEAR_AUTOMATION set) mustn't start
/// against `home`: it's the real data folder, so its runner is the live one
/// and every command a script sends would act on Mike's real threads, write
/// his drafts and settings, and start real turns. That's what an app with no
/// THREADBEAR_HOME gets (the parity audit, 13 Nov). `allowed` is
/// THREADBEAR_AUTOMATION_LIVE=1, for when the person whose runner it is says
/// so, as the Swift app's `--automation-allow-live`.
pub fn scripted_refusal(home: &Path, real: &Path, allowed: bool) -> Option<String> {
    if allowed {
        return None;
    }
    // Compared with links resolved, so ~/x/../Library/... or a link to the
    // real folder is the real folder too.
    let resolved = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    (resolved(home) == resolved(real)).then(|| {
        format!(
            "A scripted app won't use the real data folder ({}): its runner is the live one, with real threads. Set THREADBEAR_HOME to the mock's or a dev runner's (desktop/scripts/dev-app.sh does), or THREADBEAR_AUTOMATION_LIVE=1 if the person whose runner it is has said so.",
            real.display()
        )
    })
}

fn expand_home(p: &str) -> PathBuf {
    if p == "~" {
        return dirs::home_dir().unwrap_or_default();
    }
    if let Some(rest) = p.strip_prefix("~/") {
        return dirs::home_dir().unwrap_or_default().join(rest);
    }
    PathBuf::from(p)
}

fn port_from_runner_json(home: &Path) -> Option<u16> {
    let text = std::fs::read_to_string(home.join("runner.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    v.get("port")?.as_u64().and_then(|p| u16::try_from(p).ok()).filter(|p| *p > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_port_from_runner_json() {
        let dir = std::env::temp_dir().join(format!("tb-desktop-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(port_from_runner_json(&dir), None);
        std::fs::write(dir.join("runner.json"), r#"{"port": 9123, "other": true}"#).unwrap();
        assert_eq!(port_from_runner_json(&dir), Some(9123));
        std::fs::write(dir.join("runner.json"), r#"{"port": 0}"#).unwrap();
        assert_eq!(port_from_runner_json(&dir), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_scripted_app_refuses_the_real_data_folder() {
        let base = std::env::temp_dir().join(format!("tb-desktop-refuse-{}", std::process::id()));
        let real = base.join("real");
        let other = base.join("mock");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        assert!(scripted_refusal(&real, &real, false).is_some());
        // The same folder by another way there.
        assert!(scripted_refusal(&base.join("mock/../real"), &real, false).is_some());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&real, base.join("link")).unwrap();
            assert!(scripted_refusal(&base.join("link"), &real, false).is_some());
        }
        // The real folder not made yet still counts.
        assert!(scripted_refusal(&base.join("none"), &base.join("none"), false).is_some());
        assert_eq!(scripted_refusal(&other, &real, false), None);
        // Told it may.
        assert_eq!(scripted_refusal(&real, &real, true), None);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn expands_a_leading_tilde() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand_home("~/x/y"), home.join("x/y"));
        assert_eq!(expand_home("/abs"), PathBuf::from("/abs"));
    }
}

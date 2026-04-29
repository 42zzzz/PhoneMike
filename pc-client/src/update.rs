/// Background update check â€” queries GitHub releases API via PowerShell.
/// No new crate deps; PowerShell is available on all supported Windows versions.
use crate::state::AppStateHandle;

pub const CURRENT_VERSION: &str = "v1.1.1";

const RELEASES_URL: &str =
    "https://api.github.com/repos/42zzzz/PhoneMike/releases/latest";

pub fn spawn_update_check(state: AppStateHandle) {
    std::thread::spawn(move || {
        match check_for_update() {
            Some(tag) if tag != CURRENT_VERSION => {
                if let Ok(mut st) = state.lock() {
                    st.update_available = Some(tag);
                }
            }
            _ => {}
        }
    });
}

/// Returns the latest release tag (e.g. "v1.1.0") or None on any error.
fn check_for_update() -> Option<String> {
    let ps_cmd = format!(
        r#"try {{ \
            $r = Invoke-WebRequest -Uri '{url}' -UseBasicParsing \
                 -Headers @{{'User-Agent'='PhoneMike/{ver}'}} -TimeoutSec 10; \
            ($r.Content | ConvertFrom-Json).tag_name \
        }} catch {{ '' }}"#,
        url = RELEASES_URL,
        ver = CURRENT_VERSION,
    );

    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle", "Hidden",
            "-NonInteractive",
            "-Command", &ps_cmd,
        ])
        .output()
        .ok()?;

    let tag = String::from_utf8(output.stdout).ok()?;
    let tag = tag.trim().to_string();
    if tag.starts_with('v') { Some(tag) } else { None }
}

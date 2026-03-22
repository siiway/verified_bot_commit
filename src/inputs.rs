use std::env;

/// Read a GitHub Actions input from environment variables.
/// GitHub Actions sets inputs as `INPUT_<NAME>` with uppercase name and `-` replaced by `_`.
pub fn get_input(name: &str) -> String {
    let env_name = format!("INPUT_{}", name.to_uppercase().replace('-', "_"));
    env::var(&env_name).unwrap_or_default()
}

/// Read a GitHub Actions boolean input.
pub fn get_bool_input(name: &str) -> bool {
    get_input(name).eq_ignore_ascii_case("true")
}

/// Read a GitHub Actions multiline input (newline-delimited).
pub fn get_multiline_input(name: &str) -> Vec<String> {
    get_input(name)
        .lines()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Set a GitHub Actions output value.
pub fn set_output(name: &str, value: &str) {
    println!("::set-output name={name}::{value}");

    // Also write to GITHUB_OUTPUT file if available
    if let Ok(output_file) = env::var("GITHUB_OUTPUT") {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(&output_file) {
            let _ = writeln!(file, "{name}={value}");
        }
    }
}

/// Log an info message.
pub fn info(msg: &str) {
    println!("{msg}");
}

/// Log a warning.
pub fn warning(msg: &str) {
    println!("::warning::{msg}");
}

/// Log a notice.
pub fn notice(msg: &str) {
    println!("::notice::{msg}");
}

/// Log a group start.
pub fn start_group(title: &str) {
    println!("::group::{title}");
}

/// Log a group end.
pub fn end_group() {
    println!("::endgroup::");
}

/// Set the action as failed.
pub fn set_failed(msg: &str) {
    println!("::error::{msg}");
    std::process::exit(1);
}

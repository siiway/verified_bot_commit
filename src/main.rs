mod git;
mod inputs;

use glob_match::glob_match;
use std::process::Command;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        inputs::set_failed(&e);
    }
}

async fn run() -> Result<(), String> {
    // Parse inputs
    let repository = inputs::get_input("repository");
    let token = inputs::get_input("token");
    let api_url = inputs::get_input("api-url");
    let max_retries: u32 = inputs::get_input("max-retries").parse().unwrap_or(1);
    let auto_stage = inputs::get_bool_input("auto-stage");
    let update_local = inputs::get_bool_input("update-local");
    let force_push = inputs::get_bool_input("force-push");
    let allow_empty_commit = inputs::get_bool_input("allow-empty-commit");
    let follow_symlinks = inputs::get_bool_input("follow-symlinks");
    let workspace = inputs::get_input("workspace");
    let patterns = inputs::get_multiline_input("files");

    let no_commit_action = if allow_empty_commit {
        "ignore".to_string()
    } else {
        inputs::get_input("if-no-commit")
    };

    // Build commit message
    let message = git::build_commit_message(
        &inputs::get_input("message"),
        &inputs::get_input("message-file"),
    )?;

    // Parse repository
    if !repository.contains('/') {
        return Err("Repository must be in the format 'owner/name'".to_string());
    }
    let parts: Vec<&str> = repository.splitn(2, '/').collect();
    let (owner, repo) = (parts[0], parts[1]);

    // Normalize ref
    let git_ref = git::normalize_ref(&inputs::get_input("ref"));

    // Create API client
    let api = git::GitHubApi::new(&api_url, owner, repo, &token, max_retries);

    // Look up HEAD commit and tree
    let head_commit = api.get_ref(&git_ref).await?;
    let head_tree = api.get_tree(&head_commit).await?;

    // Get changed files
    inputs::start_group("Getting changed files...");

    if auto_stage {
        let status = Command::new("git")
            .args(["add", "-A"])
            .current_dir(&workspace)
            .status()
            .map_err(|e| format!("Failed to run git add: {e}"))?;
        if !status.success() {
            return Err("git add -A failed".to_string());
        }
    }

    let output = Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(&workspace)
        .output()
        .map_err(|e| format!("Failed to run git diff: {e}"))?;

    if !output.status.success() {
        return Err("git diff --cached --name-only failed".to_string());
    }

    let changed_files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .trim()
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();

    inputs::end_group();

    // Handle no changed files
    if changed_files.is_empty() {
        emit_no_commit_message(&no_commit_action, "No changes found in local branch")?;
        if !allow_empty_commit {
            return Ok(());
        }
    }

    // Create blobs for matching files
    let mut blobs: Vec<git::GitBlob> = Vec::new();

    inputs::start_group("Creating Git Blobs...");
    for file in &changed_files {
        let mut matched = false;

        for pattern in &patterns {
            // Skip blank and comment patterns
            if pattern.starts_with('#') || pattern.is_empty() {
                continue;
            }

            // Negation pattern - skip file if it matches
            if let Some(negated) = pattern.strip_prefix('!') {
                if glob_match(negated, file) {
                    break;
                }
                continue;
            }

            // Include file if it matches
            if glob_match(pattern, file) {
                matched = true;
                break;
            }
        }

        if matched {
            let blob = api.create_blob(file, &workspace, follow_symlinks).await?;
            inputs::info(&format!("{}\t{}", blob.sha, blob.path));
            blobs.push(blob);
        }
    }
    inputs::end_group();

    let blob_shas: Vec<&str> = blobs.iter().map(|b| b.sha.as_str()).collect();
    inputs::set_output(
        "blobs",
        &serde_json::to_string(&blob_shas).unwrap_or_default(),
    );

    // Create tree or reuse existing
    let tree = if blobs.is_empty() {
        emit_no_commit_message(&no_commit_action, "No files to commit")?;
        if !allow_empty_commit {
            return Ok(());
        }
        inputs::info(&format!("Reusing Git Tree @ {head_tree}"));
        head_tree.clone()
    } else {
        let tree_sha = api.create_tree(&blobs, &head_tree).await?;
        inputs::info(&format!("Created Git Tree @ {tree_sha}"));
        tree_sha
    };
    inputs::set_output("tree", &tree);

    // Create the signed commit
    let commit = api.create_commit(&tree, &head_commit, &message).await?;
    inputs::info(&format!("Created Commit @ {commit}"));
    inputs::set_output("commit", &commit);

    // Update the ref
    let ref_sha = api.update_ref(&git_ref, &commit, force_push).await?;
    inputs::info(&format!("Updated refs/{git_ref} to point to {ref_sha}"));
    inputs::set_output("ref", &ref_sha);

    // Update local ref
    if update_local {
        inputs::start_group("Updating local ref...");

        let result = if git_ref.starts_with("tags/") {
            Command::new("git")
                .args([
                    "fetch",
                    "origin",
                    &format!("refs/{git_ref}"),
                    "--tags",
                    "--force",
                ])
                .current_dir(&workspace)
                .status()
        } else {
            Command::new("git")
                .args(["pull", "origin", &format!("refs/{git_ref}")])
                .current_dir(&workspace)
                .status()
        };

        match result {
            Ok(status) if status.success() => {}
            Ok(_) => inputs::warning("Failed to update local ref"),
            Err(e) => inputs::warning(&format!("Failed to update local ref: {e}")),
        }

        inputs::end_group();
    }

    Ok(())
}

fn emit_no_commit_message(action: &str, msg: &str) -> Result<(), String> {
    match action {
        "error" => Err(msg.to_string()),
        "warning" => {
            inputs::warning(msg);
            Ok(())
        }
        "notice" => {
            inputs::notice(msg);
            Ok(())
        }
        _ => {
            inputs::info(msg);
            Ok(())
        }
    }
}

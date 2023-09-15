use std::{env::current_dir, path::Path, process::Command};

use releasy_core::{
    event::{Event, EventType},
    repo::Repo,
};

pub trait EventHandler {
    fn handle(&self, current_repo: &Repo) -> anyhow::Result<()>;
}

impl EventHandler for Event {
    fn handle(&self, current_repo: &Repo) -> anyhow::Result<()> {
        match self.event_type() {
            EventType::NewCommit => handle_new_commit(self, current_repo),
            EventType::NewRelease => handle_new_release(self),
        }
    }
}

/// Handles the case when there is a new commit to an upstream repository.
///
/// For our needs, we want to make sure that our tracking branch (which contains patches in
/// `Cargo.toml`s that causes `master` version of upstream repos to be used, instead of the latest
/// released version) runs the CI again. To run the CI again new_commit handler, pushes a new commit
/// to the tracking branch.
///
/// By default we are expecting the tracking branch to be named as:
///
/// ```
/// upgrade/<source_repo_name>-master
/// ```
fn handle_new_commit(event: &Event, current_repo: &Repo) -> anyhow::Result<()> {
    println!(
        "New commit event received from {}, commit hash: {:?}",
        event.client_payload().repo(),
        event.client_payload().details().commit_hash()
    );

    let source_repo = event.client_payload().repo();
    let commit_hash = event
        .client_payload()
        .details()
        .commit_hash()
        .ok_or_else(|| anyhow::anyhow!("target commit hash missing"))?;
    let tracking_branch_name = format!("upgrade/{}-master", source_repo.name());

    with_tmp_dir(commit_hash, |tmp_dir_path| {
        let absolute_path = tmp_dir_path.canonicalize()?;

        // Clone the repo inside a tmp directory.
        Command::new("git")
            .arg("clone")
            .arg(current_repo.github_url())
            .current_dir(&absolute_path)
            .spawn()?
            .wait()?;

        let repo_path = absolute_path.join(current_repo.name());

        // Checkout tracking branch
        Command::new("git")
            .arg("checkout")
            .arg("-b")
            .arg(&tracking_branch_name)
            .current_dir(&repo_path)
            .spawn()?
            .wait()?;

        // Pull remote changes
        Command::new("git")
            .arg("pull")
            .arg("origin")
            .arg(&tracking_branch_name)
            .current_dir(&repo_path)
            .spawn()?
            .wait()?;

        // Create an empty commit.
        let commit_message = format!(
            "re-run CI after {} commit merged to {}/{}",
            commit_hash,
            source_repo.owner(),
            source_repo.name()
        );
        Command::new("git")
            .arg("commit")
            .arg("--allow-empty")
            .arg("-m")
            .arg(format!("\"{}\"", commit_message))
            .current_dir(&repo_path)
            .spawn()?
            .wait()?;

        // Push empty commit to remote.
        Command::new("git")
            .arg("push")
            .arg("origin")
            .arg(&tracking_branch_name)
            .current_dir(&repo_path)
            .spawn()?
            .wait()?;

        Ok(())
    })?;

    // TODO: Push an empty commit to the tracking branch.
    Ok(())
}

fn handle_new_release(event: &Event) -> anyhow::Result<()> {
    println!(
        "New release event received from {}, release_tag: {:?}",
        event.client_payload().repo(),
        event.client_payload().details().release_tag()
    );
    println!("Not yet implemented!");
    Ok(())
}

/// Initializes a new temporary directory to fetch current repo into.
fn with_tmp_dir<F>(dir_name: &str, f: F) -> anyhow::Result<()>
where
    F: FnOnce(&Path) -> anyhow::Result<()>,
{
    // Clear existing temporary directory if it exists.
    let repo_dir = current_dir()?.join(".tmp").join(dir_name);
    if repo_dir.exists() {
        let _ = std::fs::remove_dir_all(&repo_dir);
    }

    // Create the tmp dir if it does not exists
    std::fs::create_dir_all(&repo_dir)?;

    // Call the user function.
    f(&repo_dir)?;

    // Clean up the temporary directory.
    let _ = std::fs::remove_dir_all(&repo_dir);
    Ok(())
}

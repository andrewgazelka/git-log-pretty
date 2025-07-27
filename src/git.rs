use eyre::{Result, WrapErr};
use git2::{Commit, DiffOptions, Oid, Repository};
use std::collections::HashSet;

pub fn collect_commits(repo: &Repository, start_id: Oid, commits: &mut HashSet<Oid>) -> Result<()> {
    let mut revwalk = repo.revwalk().wrap_err("Failed to create revwalk")?;
    revwalk
        .push(start_id)
        .wrap_err("Failed to push commit to revwalk")?;

    for oid in revwalk {
        commits.insert(oid.wrap_err("Failed to get commit OID")?);
    }

    Ok(())
}

pub fn get_changed_files(repo: &Repository, commit: &Commit) -> Result<Vec<String>> {
    let mut files = Vec::new();

    if commit.parent_count() == 0 {
        // Initial commit - compare against empty tree
        let tree = commit.tree().wrap_err("Failed to get commit tree")?;
        tree.walk(git2::TreeWalkMode::PreOrder, |_, entry| {
            if let Some(name) = entry.name() {
                files.push(name.to_string());
            }
            git2::TreeWalkResult::Ok
        })
        .wrap_err("Failed to walk tree")?;
    } else {
        // Compare with parent
        let parent = commit.parent(0).wrap_err("Failed to get parent commit")?;
        let parent_tree = parent.tree().wrap_err("Failed to get parent tree")?;
        let commit_tree = commit.tree().wrap_err("Failed to get commit tree")?;

        let mut diff_opts = DiffOptions::new();
        let diff = repo
            .diff_tree_to_tree(Some(&parent_tree), Some(&commit_tree), Some(&mut diff_opts))
            .wrap_err("Failed to create diff")?;

        diff.foreach(
            &mut |delta, _progress| {
                if let Some(path) = delta.new_file().path() {
                    if let Some(path_str) = path.to_str() {
                        files.push(path_str.to_string());
                    }
                }
                true
            },
            None,
            None,
            None,
        )
        .wrap_err("Failed to iterate diff")?;
    }

    Ok(files)
}

pub fn get_diff_stats(
    repo: &Repository,
    base_branch: &str,
    head_branch: &str,
) -> Result<Vec<String>> {
    let base_ref = repo
        .find_reference(&format!("refs/heads/{base_branch}"))
        .wrap_err(format!("Failed to find {base_branch} branch"))?;
    let base_commit = repo
        .find_commit(base_ref.target().unwrap())
        .wrap_err("Failed to find base commit")?;
    let base_tree = base_commit.tree().wrap_err("Failed to get base tree")?;

    let head_ref = if head_branch == "HEAD" {
        repo.head().wrap_err("Failed to get HEAD reference")?
    } else {
        repo.find_reference(&format!("refs/heads/{head_branch}"))
            .wrap_err(format!("Failed to find {head_branch} branch"))?
    };
    let head_commit = repo
        .find_commit(head_ref.target().unwrap())
        .wrap_err("Failed to find head commit")?;
    let head_tree = head_commit.tree().wrap_err("Failed to get head tree")?;

    let mut diff_opts = DiffOptions::new();
    let diff = repo
        .diff_tree_to_tree(Some(&base_tree), Some(&head_tree), Some(&mut diff_opts))
        .wrap_err("Failed to create diff")?;

    let mut changed_files = Vec::new();

    diff.foreach(
        &mut |delta, _progress| {
            if let Some(path) = delta.new_file().path() {
                if let Some(path_str) = path.to_str() {
                    changed_files.push(path_str.to_string());
                }
            } else if let Some(path) = delta.old_file().path() {
                if let Some(path_str) = path.to_str() {
                    changed_files.push(path_str.to_string());
                }
            }
            true
        },
        None,
        None,
        None,
    )
    .wrap_err("Failed to iterate diff")?;

    Ok(changed_files)
}

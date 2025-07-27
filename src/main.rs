use clap::{Parser, Subcommand};
use crossterm::style::{Color, Stylize};
use devicons::Theme;
use eyre::{Result, WrapErr};
use git2::{Oid, Repository};
use regex::Regex;
use std::collections::HashSet;
use terminal_light::luma;

mod colors;
mod display;
mod git;
mod icons;
mod time;

use display::print_pretty_commit;
use git::{collect_commits, get_diff_stats};
use icons::get_file_icons;

#[derive(Parser)]
#[command(name = "git-log-pretty")]
#[command(about = "A pretty git log viewer with tree views")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Show diff stats with tree view (like git diff --stat but prettier)
    Diff {
        /// Base branch to compare against
        #[arg(default_value = "main")]
        base: String,
        /// Head branch to compare (defaults to current HEAD)
        #[arg(default_value = "HEAD")]
        head: String,
    },
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Diff { base, head }) => {
            run_diff_stats(base, head).wrap_err("Failed to analyze diff stats")
        }
        None => run_git_log().wrap_err("Failed to analyze git log"),
    }
}

fn run_diff_stats(base_branch: &str, head_branch: &str) -> Result<()> {
    let repo = Repository::discover(".").wrap_err("Failed to discover git repository")?;

    let changed_files = get_diff_stats(&repo, base_branch, head_branch)?;

    if changed_files.is_empty() {
        println!("{}", "No changes found".with(Color::Green));
        return Ok(());
    }

    // Cache theme detection once
    let theme = match luma() {
        Ok(val) if val > 0.5 => Some(Theme::Light),
        _ => Some(Theme::Dark),
    };

    println!(
        "{} files changed in {}...{}\n",
        changed_files.len().to_string().with(Color::Cyan),
        base_branch.with(Color::Yellow),
        head_branch.with(Color::Yellow)
    );

    let file_icons = get_file_icons(&changed_files, &theme);

    if !file_icons.is_empty() {
        println!("{file_icons}");
    }

    println!();

    Ok(())
}

fn run_git_log() -> Result<()> {
    let repo = Repository::discover(".").wrap_err("Failed to discover git repository")?;

    let main_ref = repo
        .find_reference("refs/heads/main")
        .wrap_err("Failed to find main branch")?;
    let main_commit_id = main_ref.target().unwrap();

    let head_ref = repo.head().wrap_err("Failed to get HEAD reference")?;
    let head_commit_id = head_ref.target().unwrap();

    if main_commit_id == head_commit_id {
        println!("{}", "All caught up with main".with(Color::Green));
        return Ok(());
    }

    let mut main_commits = HashSet::new();
    collect_commits(&repo, main_commit_id, &mut main_commits)?;

    let mut head_commits = HashSet::new();
    collect_commits(&repo, head_commit_id, &mut head_commits)?;

    let mut ahead_commits: Vec<Oid> = head_commits.difference(&main_commits).cloned().collect();

    ahead_commits.sort_by(|a, b| {
        let commit_a = repo.find_commit(*a).unwrap();
        let commit_b = repo.find_commit(*b).unwrap();
        commit_b.time().seconds().cmp(&commit_a.time().seconds())
    });

    if ahead_commits.is_empty() {
        println!("{}", "All caught up with main".with(Color::Green));
        return Ok(());
    }

    // Cache theme detection once
    let theme = match luma() {
        Ok(val) if val > 0.5 => Some(Theme::Light),
        _ => Some(Theme::Dark),
    };

    // Compile regex once for conventional commits
    let conventional_commit_regex = Regex::new(r"^([A-Za-z]+)(?:\(([^)]+)\))?:(.*)$").unwrap();

    // Limit to first 15 commits for performance
    let display_commits = &ahead_commits[..ahead_commits.len().min(15)];
    let hidden_count = ahead_commits.len().saturating_sub(15);

    println!(
        "{} commits ahead of main{}\n",
        ahead_commits.len().to_string().with(Color::Cyan),
        if hidden_count > 0 {
            format!(" (showing first 15, {hidden_count} more hidden)")
                .with(Color::DarkGrey)
                .to_string()
        } else {
            String::new()
        }
    );

    for commit_id in display_commits {
        let commit = repo
            .find_commit(*commit_id)
            .wrap_err("Failed to find commit")?;
        print_pretty_commit(&repo, &commit, &theme, &conventional_commit_regex)?;
    }

    Ok(())
}
// test comment

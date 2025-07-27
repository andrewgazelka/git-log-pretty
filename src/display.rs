use crate::colors::hash_to_background_color;
use crate::git::get_changed_files;
use crate::icons::get_file_icons;
use crate::time::format_time;
use crossterm::style::{Color, Stylize};
use devicons::Theme;
use eyre::Result;
use git2::{Commit, Repository};
use regex::Regex;

pub fn print_pretty_commit(
    repo: &Repository,
    commit: &Commit,
    theme: &Option<Theme>,
    regex: &Regex,
) -> Result<()> {
    let id = commit.id();
    let short_id = &id.to_string()[0..7];
    let message = commit.message().unwrap_or("<no message>");
    let summary = message.lines().next().unwrap_or("").trim();
    let time = format_time(&commit.time());

    // Get file changes
    let changed_files = get_changed_files(repo, commit)?;
    let file_icons = get_file_icons(&changed_files, theme);

    // Parse conventional commit prefix and apply background color
    let formatted_summary = format_conventional_commit(summary, regex, theme);

    // Compact output - commit info and time on same line
    println!(
        "  {} {} • {}",
        short_id.with(Color::Yellow),
        formatted_summary,
        time.with(Color::DarkGrey)
    );

    if !file_icons.is_empty() {
        println!("{file_icons}");
    }

    println!();

    Ok(())
}

pub fn format_conventional_commit(
    summary: &str,
    re: &Regex,
    theme: &Option<devicons::Theme>,
) -> String {
    if let Some(captures) = re.captures(summary) {
        let commit_type = captures.get(1).unwrap().as_str();
        let scope = captures.get(2).map(|m| m.as_str());
        let description = captures.get(3).unwrap().as_str();

        // Determine if we're on a dark theme for better color contrast
        let is_dark_theme = matches!(theme, Some(devicons::Theme::Dark) | None);

        // Hash the commit type to get a consistent color
        let bg_color = hash_to_background_color(commit_type, is_dark_theme);

        let formatted_type = commit_type.on(bg_color).with(Color::White).bold();

        if let Some(scope_text) = scope {
            format!(
                "{} {}{}",
                formatted_type,
                scope_text.with(Color::DarkGrey),
                description
            )
        } else {
            format!("{formatted_type}{description}")
        }
    } else {
        summary.to_string()
    }
}

use crate::colors::hash_to_background_color;
use crate::git::get_changed_files;
use crate::icons::get_file_icons;
use crate::time::format_time;
use crossterm::style::{Color, Stylize};
use devicons::Theme;
use eyre::Result;
use git2::{Commit, Repository};
use regex::Regex;

/// Build the text rows for one commit: a header line (short id, summary, age)
/// followed by the file-change tree. The rows carry their own indentation, so
/// the caller prints them verbatim (or shifts them right to clear an avatar).
pub fn commit_lines(
    repo: &Repository,
    commit: &Commit,
    theme: &Option<Theme>,
    regex: &Regex,
) -> Result<Vec<String>> {
    let id = commit.id();
    let short_id = &id.to_string()[0..7];
    let message = commit.message().unwrap_or("<no message>");
    let summary = message.lines().next().unwrap_or("").trim();
    let time = format_time(&commit.time());

    let changed_files = get_changed_files(repo, commit)?;
    let file_icons = get_file_icons(&changed_files, theme);

    let formatted_summary = format_conventional_commit(summary, regex, theme);

    let mut lines = vec![format!(
        "  {} {} • {}",
        short_id.with(Color::Yellow),
        formatted_summary,
        time.with(Color::DarkGrey)
    )];

    if !file_icons.is_empty() {
        lines.extend(file_icons.split('\n').map(str::to_string));
    }

    Ok(lines)
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

        // Use appropriate text color based on theme
        let text_color = if is_dark_theme {
            Color::White // White text on dark backgrounds
        } else {
            Color::Black // Black text on light backgrounds
        };

        let formatted_type = commit_type.on(bg_color).with(text_color).bold();

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

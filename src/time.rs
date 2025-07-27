use git2::Time;

pub fn format_time(time: &Time) -> String {
    let timestamp = time.seconds();
    let datetime = chrono::DateTime::from_timestamp(timestamp, 0).unwrap();
    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(datetime);

    if duration.num_days() > 0 {
        format!("{} days ago", duration.num_days())
    } else if duration.num_hours() > 0 {
        format!("{} hours ago", duration.num_hours())
    } else if duration.num_minutes() > 0 {
        format!("{} minutes ago", duration.num_minutes())
    } else {
        "just now".to_string()
    }
}

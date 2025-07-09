use notify_rust::{Hint, Notification, Timeout};

pub async fn notify(summary: &str, body: &str, priority_info: &str) -> i16 {
    // https://specifications.freedesktop.org/icon-naming-spec/latest/ar01s04.html
    Notification::new()
        .action("Left", "Left")
        .action("Center", "Center")
        .action("Right", "Right")
        .summary(summary)
        .body(body)
        .icon(priority_info)
        .hint(Hint::SoundName(priority_info.to_string()))
        .timeout(Timeout::Never)
        .show_async()
        .await
        .unwrap();
    0
}

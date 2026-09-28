//! Pure request-policy helpers, also tested without the Aidoku/WASM runtime.

pub const SEARCH_COOLDOWN_SECONDS: i64 = 6;

pub fn search_wait(now: i64, last: i64) -> i64 {
    // A clock correction or stale preference must not lock search indefinitely.
    if last <= 0 || last > now {
        return 0;
    }
    SEARCH_COOLDOWN_SECONDS.saturating_sub(now.saturating_sub(last)).max(0)
}

pub fn url_path(url: &str) -> &str {
    let url = url.split(['?', '#']).next().unwrap_or("");
    match url.split_once("://") {
        Some((_, rest)) => rest.find('/').map(|index| &rest[index..]).unwrap_or("/"),
        None => url,
    }
}

pub fn is_search_url(url: &str) -> bool {
    matches!(url_path(url), "/so.php" | "/modules/article/search.php")
}

pub fn is_login_url(url: &str) -> bool {
    url_path(url) == "/login.php"
}

pub fn is_challenge_header(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("challenge")
}

pub fn is_challenge_title(title: &str) -> bool {
    let title = title.trim();
    title.eq_ignore_ascii_case("just a moment...")
        || title.eq_ignore_ascii_case("just a moment…")
        || title.eq_ignore_ascii_case("attention required! | cloudflare")
        || title.eq_ignore_ascii_case("checking your browser...")
}

pub fn is_search_throttled(message: &str) -> bool {
    message.contains("间隔时间不得少于") || message.contains("間隔時間不得少於")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooldown_boundaries() {
        assert_eq!(search_wait(100, 100), 6);
        assert_eq!(search_wait(105, 100), 1);
        assert_eq!(search_wait(106, 100), 0);
        assert_eq!(search_wait(200, 100), 0);
    }

    #[test]
    fn clock_changes_and_missing_state_do_not_lock_search() {
        assert_eq!(search_wait(100, 101), 0);
        assert_eq!(search_wait(100, 0), 0);
        assert_eq!(search_wait(i64::MAX, 1), 0);
    }

    #[test]
    fn search_includes_post_and_pagination_not_categories() {
        assert!(is_search_url("https://www.wenku8.net/so.php"));
        assert!(is_search_url("https://www.wenku8.cc/modules/article/search.php?page=2"));
        assert!(!is_search_url("https://www.wenku8.net/modules/article/toplist.php"));
    }

    #[test]
    fn only_actual_login_path_is_login() {
        assert!(is_login_url("https://www.wenku8.net/login.php?jumpurl=abc"));
        assert!(!is_login_url("https://www.wenku8.net/book/1.htm?next=/login.php"));
        assert!(!is_login_url("https://www.wenku8.net?next=/login.php"));
        assert!(!is_login_url("https://www.wenku8.net/login.php.fake"));
    }

    #[test]
    fn challenge_detection_is_specific() {
        assert!(is_challenge_header(" Challenge "));
        assert!(!is_challenge_header("not-a-challenge"));
        assert!(is_challenge_title("Just a moment..."));
        assert!(!is_challenge_title("小说：Cloudflare 的故事"));
        assert!(!is_challenge_title("Just a moment in my life"));
    }

    #[test]
    fn recognizes_site_search_limits() {
        assert!(is_search_throttled("两次搜索间隔时间不得少于 5 秒"));
        assert!(is_search_throttled("間隔時間不得少於"));
        assert!(!is_search_throttled("没有搜索结果"));
    }
}

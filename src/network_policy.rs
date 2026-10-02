//! Pure request-policy helpers, also tested without the Aidoku/WASM runtime.

pub const SEARCH_COOLDOWN_SECONDS: i64 = 6;

pub const DEFAULT_DOWNLOAD_REQUESTS_PER_SECOND: i32 = 5;
pub const MAX_DOWNLOAD_REQUESTS_PER_SECOND: i32 = 10;
pub const MAX_CHAPTER_RATE_LIMIT_RETRIES: u32 = 2;
pub const MAX_AUTOMATIC_COOLDOWN_SECONDS: i64 = 30;
pub const DEFAULT_RETRY_AFTER_SECONDS: i64 = 10;

/// Accept Retry-After's seconds form or an HTTP-date parsed by the host.
/// Keep a one-second margin because the host clock returns whole seconds.
pub fn retry_after_seconds(now: i64, header: Option<&str>, http_date: Option<i64>) -> i64 {
    let seconds = header
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(|value| i64::try_from(value).unwrap_or(i64::MAX))
        .or_else(|| http_date.map(|date| date.saturating_sub(now).max(0)))
        .unwrap_or(DEFAULT_RETRY_AFTER_SECONDS);
    seconds.saturating_add(1)
}

pub fn server_cooldown_wait(now: i64, until: i64) -> i64 {
    until.saturating_sub(now).max(0)
}

pub fn should_retry_chapter(status: i32, challenge: bool, login_redirect: bool, retries: u32) -> bool {
    status == 429 && !challenge && !login_redirect && retries < MAX_CHAPTER_RATE_LIMIT_RETRIES
}

fn url_host(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("https") && !scheme.eq_ignore_ascii_case("http") {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') { return None; }
    Some(authority.split(':').next().unwrap_or(""))
}

/// Foreign image hosts must not inherit Wenku8's Referer.
pub fn is_wenku8_image_url(url: &str) -> bool {
    let Some(host) = url_host(url) else { return false; };
    ["wenku8.net", "wenku8.cc", "wenku8.com"].iter().any(|domain| {
        host.eq_ignore_ascii_case(domain)
            || host.len() > domain.len()
                && host.as_bytes()[host.len() - domain.len() - 1] == b'.'
                && host.get(host.len() - domain.len()..)
                    .map(|suffix| suffix.eq_ignore_ascii_case(domain)).unwrap_or(false)
    })
}

pub fn is_tencent_doc_image_url(url: &str) -> bool {
    let Some(host) = url_host(url) else { return false; };
    let suffix = ".docs.qq.com";
    if host.len() <= suffix.len() { return false; }
    let prefix_length = host.len() - suffix.len();
    if !host.get(prefix_length..).map(|value| value.eq_ignore_ascii_case(suffix)).unwrap_or(false) {
        return false;
    }
    let Some(prefix) = host.get(..prefix_length) else { return false; };
    if !prefix.get(..6).map(|value| value.eq_ignore_ascii_case("docimg")).unwrap_or(false) {
        return false;
    }
    prefix.get(6..).map(|number| !number.is_empty() && number.bytes().all(|c| c.is_ascii_digit())).unwrap_or(false)
}

pub fn image_referer<'a>(url: &str, wenku8_base_url: &'a str) -> Option<&'a str> {
    if is_tencent_doc_image_url(url) {
        Some("https://docs.qq.com/")
    } else if is_wenku8_image_url(url) {
        Some(wenku8_base_url)
    } else {
        None
    }
}

/// Zero permits disables Aidoku's source-wide rate limiter.
/// This is a request frequency limit, not a limit on in-flight downloads.
pub fn download_request_permits(enabled: Option<bool>, value: Option<&str>) -> i32 {
    if enabled == Some(false) {
        return 0;
    }
    value
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|value| (1..=MAX_DOWNLOAD_REQUESTS_PER_SECOND).contains(value))
        .unwrap_or(DEFAULT_DOWNLOAD_REQUESTS_PER_SECOND)
}

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

/// Only a real detail URL may use the single-search-result shortcut.
/// List pages also contain add-to-bookshelf links, so those are not evidence.
pub fn single_search_book_key(is_search: bool, url: &str) -> Option<&str> {
    if !is_search {
        return None;
    }
    let key = url_path(url).strip_prefix("/book/")?.strip_suffix(".htm")?;
    if key.is_empty() || !key.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(key)
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
    fn tencent_document_images_use_the_image_hosts_referer() {
        for url in [
            "https://docimg10.docs.qq.com/image/a.png",
            "https://docimg9.docs.qq.com/image/a.jpeg",
            "https://DOCIMG2.DOCS.QQ.COM/image/a.png",
        ] {
            assert_eq!(image_referer(url, "https://www.wenku8.net"), Some("https://docs.qq.com/"));
        }
    }

    #[test]
    fn image_referer_does_not_leak_to_unrelated_hosts() {
        for url in [
            "https://docimg10.docs.qq.com.evil.test/image/a.png",
            "https://docimg10.docs.qq.com@evil.test/image/a.png",
            "https://example.com/image/a.png?next=docimg10.docs.qq.com",
            "https://docimgfake.docs.qq.com/image/a.png",
            "https://docimg10.evil.test/image/a.png",
        ] {
            assert_eq!(image_referer(url, "https://www.wenku8.net"), None);
        }
        assert_eq!(image_referer("https://img.wenku8.com/image/a.jpg", "https://www.wenku8.cc"), Some("https://www.wenku8.cc"));
    }

    #[test]
    fn respects_retry_after_seconds_and_http_dates() {
        assert_eq!(retry_after_seconds(100, Some(" 10 "), None), 11);
        assert_eq!(retry_after_seconds(100, Some("0"), None), 1);
        assert_eq!(retry_after_seconds(100, Some("HTTP date"), Some(120)), 21);
        assert_eq!(retry_after_seconds(100, Some("HTTP date"), Some(99)), 1);
        assert_eq!(retry_after_seconds(100, None, None), 11);
        assert_eq!(retry_after_seconds(100, Some("invalid"), None), 11);
        assert_eq!(retry_after_seconds(100, Some("-1"), None), 11);
        assert_eq!(retry_after_seconds(100, Some("18446744073709551615"), None), i64::MAX);
    }

    #[test]
    fn server_cooldown_expires_without_early_requests() {
        assert_eq!(server_cooldown_wait(100, 111), 11);
        assert_eq!(server_cooldown_wait(110, 111), 1);
        assert_eq!(server_cooldown_wait(111, 111), 0);
        assert_eq!(server_cooldown_wait(112, 111), 0);
        assert_eq!(server_cooldown_wait(100, 0), 0);
    }

    #[test]
    fn retries_only_rate_limits_with_a_finite_budget() {
        assert!(should_retry_chapter(429, false, false, 0));
        assert!(should_retry_chapter(429, false, false, 1));
        assert!(!should_retry_chapter(429, false, false, 2));
        assert!(!should_retry_chapter(429, true, false, 0));
        assert!(!should_retry_chapter(429, false, true, 0));
        for status in [200, 401, 403, 404, 500, 503] {
            assert!(!should_retry_chapter(status, false, false, 0));
        }
    }

    #[test]
    fn only_wenku8_images_receive_the_site_referer() {
        for url in ["https://img.wenku8.com/image/1.jpg", "https://www.wenku8.net/a.png", "https://WENKU8.CC/a.jpg"] {
            assert!(is_wenku8_image_url(url), "{url}");
        }
        for url in [
            "https://docimg10.docs.qq.com/image/a.png",
            "https://example.com/image.jpg?origin=wenku8.net",
            "https://wenku8.net.example.com/a.png",
            "https://fakewenku8.net/a.png",
            "https://wenku8.net@evil.test/a.png",
            "data:image/png;base64,abc",
        ] {
            assert!(!is_wenku8_image_url(url), "{url}");
        }
    }

    #[test]
    fn download_limit_defaults_and_valid_values() {
        assert_eq!(download_request_permits(None, None), 5);
        assert_eq!(download_request_permits(Some(true), Some("1")), 1);
        assert_eq!(download_request_permits(Some(true), Some("5")), 5);
        assert_eq!(download_request_permits(Some(true), Some("10")), 10);
    }

    #[test]
    fn invalid_download_limit_uses_default() {
        for value in ["0", "-1", "11", "999999999999", "", "invalid", "2.5"] {
            assert_eq!(download_request_permits(Some(true), Some(value)), 5);
        }
    }

    #[test]
    fn disabling_download_limit_clears_permits() {
        for value in [None, Some("5"), Some("invalid")] {
            assert_eq!(download_request_permits(Some(false), value), 0);
        }
    }

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

    #[test]
    fn categories_never_use_single_book_shortcut() {
        for url in [
            "https://www.wenku8.net/modules/article/articlelist.php?page=1",
            "https://www.wenku8.net/modules/article/toplist.php?sort=allvisit",
            "https://www.wenku8.net/modules/article/tags.php?t=test",
            "https://www.wenku8.net/book/123.htm",
        ] {
            assert_eq!(single_search_book_key(false, url), None);
        }
    }

    #[test]
    fn only_search_redirect_to_numeric_detail_is_single_book() {
        assert_eq!(single_search_book_key(true, "https://www.wenku8.net/book/123.htm"), Some("123"));
        assert_eq!(single_search_book_key(true, "https://www.wenku8.cc/book/123.htm?from=search"), Some("123"));
        for url in [
            "https://www.wenku8.net/modules/article/search.php?searchkey=test",
            "https://www.wenku8.net/modules/article/articlelist.php?page=1",
            "https://www.wenku8.net/so.php?next=/book/123.htm",
            "https://www.wenku8.net/book/.htm",
            "https://www.wenku8.net/book/not-a-book.htm",
            "https://www.wenku8.net/book/123/4.htm",
        ] {
            assert_eq!(single_search_book_key(true, url), None);
        }
    }
}

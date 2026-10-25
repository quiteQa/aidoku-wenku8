//! Wenku8 identifiers and URLs, independent of the Aidoku host.
extern crate alloc;

use alloc::{format, string::String};

fn valid_id(value: &str) -> Option<&str> {
    if value.is_empty() || !value.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let id = value.parse::<u32>().ok()?;
    (id > 0).then(|| value.trim_start_matches('0'))
}

/// Return only paths on supported reader sites. Image domains are deliberately excluded.
fn site_path(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("https") && !scheme.eq_ignore_ascii_case("http") {
        return None;
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = &rest[..end];
    if !["wenku8.net", "www.wenku8.net", "wenku8.cc", "www.wenku8.cc"]
        .iter().any(|allowed| host.eq_ignore_ascii_case(allowed))
    {
        return None;
    }
    let path = &rest[end..];
    if !path.starts_with('/') || path.contains('\\') || path.chars().any(char::is_control) {
        return None;
    }
    Some(path.split('#').next().unwrap_or(path))
}

fn query_id<'a>(query: &'a str, name: &str) -> Option<&'a str> {
    let mut values = query.split('&').filter_map(|part| part.split_once('='))
        .filter(|(key, _)| *key == name);
    let (_, value) = values.next()?;
    // Reject ambiguous duplicate parameters rather than disagreeing with the server.
    if values.next().is_some() {
        return None;
    }
    valid_id(value)
}

fn static_reader(path: &str) -> Option<(&str, &str)> {
    let mut parts = path.strip_prefix("/novel/")?.split('/');
    let directory = parts.next()?;
    let book = valid_id(parts.next()?)?;
    let file = parts.next()?;
    if parts.next().is_some() || directory.parse::<u32>().ok()? != book.parse::<u32>().ok()? / 1000 {
        return None;
    }
    Some((book, file))
}

fn book_path(path: &str) -> Option<&str> {
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    if let Some(id) = path.strip_prefix("/book/").and_then(|s| s.strip_suffix(".htm")) {
        return valid_id(id);
    }
    match path {
        "/modules/article/articleinfo.php" => query_id(query, "id"),
        "/modules/article/reader.php" => query_id(query, "aid"),
        _ => {
            let (book, file) = static_reader(path)?;
            (file == "index.htm" || file.strip_suffix(".htm").and_then(valid_id).is_some())
                .then_some(book)
        }
    }
}

/// Search accepts a positive book ID or an exact Wenku8 detail/catalog/chapter URL.
pub fn book_id(input: &str) -> Option<&str> {
    let input = input.trim();
    valid_id(input).or_else(|| book_path(site_path(input)?))
}

/// Existing library chapters may still contain the previously selected site.
pub fn chapter_book_id(url: &str) -> Option<&str> {
    let path = site_path(url)?;
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    if path == "/modules/article/reader.php" {
        query_id(query, "cid")?;
        return query_id(query, "aid");
    }
    let (book, file) = static_reader(path)?;
    file.strip_suffix(".htm").and_then(valid_id)?;
    Some(book)
}

pub fn chapter_url(url: &str, base_url: &str) -> Option<String> {
    chapter_book_id(url)?;
    Some(format!("{base_url}{}", site_path(url)?))
}

pub fn cover_directory(key: &str) -> u32 {
    key.parse::<u32>().unwrap_or(0) / 1000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ids_and_supported_book_links() {
        for input in [
            "3057", " 003057 ", "https://www.wenku8.net/book/3057.htm?from=share#top",
            "http://wenku8.cc/modules/article/articleinfo.php?id=3057",
            "https://www.wenku8.net/modules/article/reader.php?aid=3057",
            "https://www.wenku8.cc/novel/3/3057/index.htm",
            "https://www.wenku8.net/novel/3/3057/176892.htm",
        ] {
            assert_eq!(book_id(input), Some("3057"), "{input}");
        }
    }

    #[test]
    fn rejects_unrelated_and_ambiguous_book_links() {
        for input in [
            "", "0", "-1", "3057oops", "4294967296",
            "https://evil.test/book/3057.htm",
            "https://www.wenku8.net.evil.test/book/3057.htm",
            "https://www.wenku8.net@evil.test/book/3057.htm",
            "https://www.wenku8.net/book/3057.htm/extra",
            "https://www.wenku8.net/book/abc.htm",
            "https://www.wenku8.net/so.php?id=3057",
            "https://www.wenku8.net/modules/article/reader.php?aid=3057bad",
            "https://www.wenku8.net/modules/article/reader.php?aid=3057&aid=1",
            "https://www.wenku8.net/novel/2/3057/index.htm",
        ] {
            assert_eq!(book_id(input), None, "{input}");
        }
    }

    #[test]
    fn rebases_only_recognized_chapters_to_the_selected_site() {
        assert_eq!(
            chapter_url("http://www.wenku8.net/modules/article/reader.php?aid=3057&cid=176892#top", "https://www.wenku8.cc"),
            Some(String::from("https://www.wenku8.cc/modules/article/reader.php?aid=3057&cid=176892"))
        );
        assert_eq!(
            chapter_url("https://wenku8.cc/novel/3/3057/176892.htm", "https://www.wenku8.net"),
            Some(String::from("https://www.wenku8.net/novel/3/3057/176892.htm"))
        );
        for url in [
            "https://evil.test/novel/3/3057/176892.htm",
            "https://www.wenku8.net/login.php?aid=3057&cid=176892",
            "https://www.wenku8.net/novel/3/3057/index.htm",
            "https://www.wenku8.net/modules/article/reader.php?aid=3057",
        ] {
            assert_eq!(chapter_url(url, "https://www.wenku8.cc"), None);
        }
    }

    #[test]
    fn cover_directories_follow_thousand_book_groups() {
        for (id, expected) in [("999", 0), ("1000", 1), ("3057", 3), ("9999", 9), ("10000", 10), ("123456", 123)] {
            assert_eq!(cover_directory(id), expected);
        }
    }
}

#![no_std]

use aidoku::{
    alloc::{format, string::ToString, vec, String, Vec},
    helpers::uri::encode_uri_component,
    imports::{html::Document, net::Request},
    prelude::*,
    Chapter, ContentRating, FilterValue, Manga, MangaPageResult, MangaStatus, Page, PageContent,
    Result, Source, Viewer,
};

const BASE_URL: &str = "https://www.wenku8.net";
const USER_AGENT: &str =
    "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 \
     (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1";

struct Wenku8;

impl Wenku8 {
    fn request_html(&self, url: &str) -> Result<Document> {
        Ok(Request::get(url)?
            .header("User-Agent", USER_AGENT)
            .header("Referer", BASE_URL)
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.5")
            .html()?)
    }

    fn book_url(key: &str) -> String {
        if key.starts_with("http://") || key.starts_with("https://") {
            key.to_string()
        } else {
            format!("{BASE_URL}/book/{key}.htm")
        }
    }

    fn reader_url(key: &str) -> String {
        // Wenku8 的阅读目录入口长期使用 reader.php?aid=<id>
        format!("{BASE_URL}/modules/article/reader.php?aid={key}")
    }

    fn extract_book_key(url: &str) -> Option<String> {
        // 兼容：
        // /book/1234.htm
        // https://www.wenku8.net/book/1234.htm
        if let Some(pos) = url.find("/book/") {
            let tail = &url[pos + 6..];
            let key = tail.split('.').next()?.split('/').next()?;
            if !key.is_empty() {
                return Some(key.to_string());
            }
        }

        // 兼容 articleinfo.php?id=1234 / reader.php?aid=1234
        for marker in ["?id=", "&id=", "?aid=", "&aid="] {
            if let Some(pos) = url.find(marker) {
                let tail = &url[pos + marker.len()..];
                let key = tail
                    .split('&')
                    .next()
                    .unwrap_or(tail)
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>();
                if !key.is_empty() {
                    return Some(key);
                }
            }
        }
        None
    }

    fn first_text(html: &Document, selectors: &[&str]) -> Option<String> {
        for selector in selectors {
            if let Some(text) = html
                .select_first(selector)
                .and_then(|el| el.text())
                .map(|s| s.trim().to_string())
            {
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
        None
    }

    fn first_attr(html: &Document, selectors: &[&str], attr: &str) -> Option<String> {
        for selector in selectors {
            if let Some(value) = html
                .select_first(selector)
                .and_then(|el| el.attr(attr))
                .map(|s| s.trim().to_string())
            {
                if !value.is_empty() {
                    return Some(value);
                }
            }
        }
        None
    }

    fn clean_label(text: String, labels: &[&str]) -> String {
        let mut s = text.trim().to_string();
        for label in labels {
            if let Some(rest) = s.strip_prefix(label) {
                s = rest.trim().to_string();
            }
        }
        s
    }

    fn parse_author(html: &Document) -> Option<String> {
        // Wenku8 不同布局里作者字段位置有差异，优先精确选择器，再做文本兜底。
        if let Some(text) = Self::first_text(
            html,
            &[
                "td:contains(小说作者)",
                "td:contains(作者)",
                "#content td:contains(小说作者)",
            ],
        ) {
            let author = Self::clean_label(text, &["小说作者：", "小说作者:", "作者：", "作者:"]);
            if !author.is_empty() {
                return Some(author);
            }
        }
        None
    }

    fn parse_description(html: &Document) -> Option<String> {
        // 常见 Wenku8 详情页中“内容简介”位于 info 表格后半部分。
        for selector in [
            "td:contains(内容简介) span",
            "td:contains(内容简介)",
            "#content td[width='48%']",
            "#content",
        ] {
            if let Some(text) = html
                .select_first(selector)
                .and_then(|el| el.text())
                .map(|s| s.trim().to_string())
            {
                if text.len() > 20 {
                    let cleaned = text
                        .replace("内容简介", "")
                        .replace("内容简介：", "")
                        .trim()
                        .to_string();
                    if !cleaned.is_empty() {
                        return Some(cleaned);
                    }
                }
            }
        }
        None
    }

    fn parse_search_results(&self, html: &Document) -> Vec<Manga> {
        let mut entries: Vec<Manga> = Vec::new();
        let mut seen: Vec<String> = Vec::new();

        if let Some(links) = html.select("a[href*='/book/']") {
            for link in links {
                let Some(url) = link.attr("abs:href").or_else(|| link.attr("href")) else {
                    continue;
                };
                let Some(key) = Self::extract_book_key(&url) else {
                    continue;
                };
                if seen.iter().any(|v| v == &key) {
                    continue;
                }

                let title = link
                    .attr("title")
                    .or_else(|| link.text())
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();

                if title.is_empty() {
                    continue;
                }

                seen.push(key.clone());
                entries.push(Manga {
                    key,
                    title,
                    url: Some(if url.starts_with("http") {
                        url
                    } else {
                        format!("{BASE_URL}{url}")
                    }),
                    ..Default::default()
                });
            }
        }

        entries
    }

    fn chapter_text(&self, chapter: &Chapter) -> Result<String> {
        let url = chapter
            .url
            .clone()
            .unwrap_or_else(|| chapter.key.clone());

        let html = self.request_html(&url)?;

        for selector in ["#acontent", "#content", "div#content"] {
            if let Some(container) = html.select_first(selector) {
                // text() 在 Aidoku/SwiftSoup 中会处理 HTML 实体；
                // 对小说正文比直接保留 HTML 更适合 Text Reader。
                if let Some(text) = container.text() {
                    let text = text
                        .replace("\r\n", "\n")
                        .replace('\r', "\n")
                        .trim()
                        .to_string();

                    if !text.is_empty() {
                        return Ok(text);
                    }
                }
            }
        }

        bail!("没有找到章节正文（#acontent / #content）");
    }
}

impl Source for Wenku8 {
    fn new() -> Self {
        Self
    }

    fn get_search_manga_list(
        &self,
        query: Option<String>,
        page: i32,
        _filters: Vec<FilterValue>,
    ) -> Result<MangaPageResult> {
        let url = if let Some(query) = query.filter(|q| !q.trim().is_empty()) {
            format!(
                "{BASE_URL}/modules/article/search.php?searchtype=articlename&searchkey={}&page={}&charset=utf-8",
                encode_uri_component(query.trim()),
                page
            )
        } else {
            // 无搜索词时显示最近更新。
            format!(
                "{BASE_URL}/modules/article/toplist.php?sort=lastupdate&page={}",
                page
            )
        };

        let html = self.request_html(&url)?;
        let entries = self.parse_search_results(&html);

        // Wenku8 搜索页通常每页有固定数量结果。
        // 当前无法从外部环境稳定访问站点验证分页控件，所以这里用“本页非空”作为保守判断。
        let has_next_page = !entries.is_empty();

        Ok(MangaPageResult {
            entries,
            has_next_page,
        })
    }

    fn get_manga_update(
        &self,
        mut manga: Manga,
        needs_details: bool,
        needs_chapters: bool,
    ) -> Result<Manga> {
        let book_url = Self::book_url(&manga.key);

        if needs_details {
            let html = self.request_html(&book_url)?;

            if let Some(title) = Self::first_text(
                &html,
                &[
                    "td[align='center'][valign='middle'] b",
                    "td[valign='middle'][align='center'] b",
                    "h1",
                    "title",
                ],
            ) {
                // title 标签往往带站点后缀，因此仅在没有更好的标题时采用。
                if !title.is_empty() {
                    manga.title = title;
                }
            }

            manga.cover = Self::first_attr(
                &html,
                &["img[vspace]", "#content img", "table img"],
                "abs:src",
            )
            .or_else(|| {
                Self::first_attr(&html, &["img[vspace]", "#content img", "table img"], "src")
            });

            manga.authors = Self::parse_author(&html).map(|a| vec![a]);
            manga.description = Self::parse_description(&html);
            manga.url = Some(book_url.clone());
            manga.content_rating = ContentRating::Safe;
            manga.status = MangaStatus::Unknown;
            manga.viewer = Viewer::RightToLeft;
        }

        if needs_chapters {
            let html = self.request_html(&Self::reader_url(&manga.key))?;
            let mut chapters: Vec<Chapter> = Vec::new();

            if let Some(links) = html.select("td.ccss a[href], .ccss a[href]") {
                for (index, link) in links.enumerate() {
                    let Some(href) = link.attr("abs:href").or_else(|| link.attr("href")) else {
                        continue;
                    };

                    let url = if href.starts_with("http://") || href.starts_with("https://") {
                        href
                    } else {
                        format!("{BASE_URL}/{href}")
                    };

                    let title = link
                        .text()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());

                    chapters.push(Chapter {
                        key: url.clone(),
                        title,
                        chapter_number: Some(index as f32 + 1.0),
                        url: Some(url),
                        ..Default::default()
                    });
                }
            }

            if chapters.is_empty() {
                bail!("没有解析到章节目录；Wenku8 可能要求登录，或页面结构已变化");
            }

            // Aidoku 一般按新 -> 旧显示章节；Wenku8 reader 页面常为旧 -> 新。
            chapters.reverse();
            manga.chapters = Some(chapters);
        }

        Ok(manga)
    }

    fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
        let text = self.chapter_text(&chapter)?;

        Ok(vec![Page {
            content: PageContent::text(text),
            ..Default::default()
        }])
    }
}

register_source!(Wenku8);

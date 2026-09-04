#![no_std]

use aidoku::{
    alloc::{format, string::ToString, vec, String, Vec},
    helpers::uri::encode_uri_component,
    imports::{
        defaults::{defaults_get, defaults_get_map, defaults_set, DefaultValue},
        html::Document,
        net::{Request, Response},
    },
    prelude::*,
    Chapter, ContentRating, FilterValue, HashMap, Manga, MangaPageResult, MangaStatus, Page,
    PageContent, Result, Source, Viewer, WebLoginHandler,
};

const DEFAULT_SITE: &str = "wenku8.net";
const SITE_SETTING_KEY: &str = "wenku8_site";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36 Edg/135.0.0.0";
const LOGIN_NET_KEY: &str = "wenku8_login_net";
const LOGIN_CC_KEY: &str = "wenku8_login_cc";
const AUTH_COOKIE_STORAGE_PREFIX: &str = "wenku8_auth_cookies_";
const LOGIN_COOKIE_NAME: &str = "jieqiUserInfo";
const VISIT_COOKIE_NAME: &str = "jieqiVisitInfo";

struct Wenku8;

impl Wenku8 {
    fn base_url(&self) -> String {
        format!("https://{}", self.selected_site())
    }

    fn auth_cookie_storage_key(&self) -> String {
        format!("{AUTH_COOKIE_STORAGE_PREFIX}{}", self.selected_site())
    }

    fn selected_site(&self) -> String {
        defaults_get::<String>(SITE_SETTING_KEY)
            .filter(|value| value == "wenku8.net" || value == "wenku8.cc")
            .unwrap_or_else(|| DEFAULT_SITE.to_string())
    }

    fn cookie_header(&self) -> Option<String> {
        let cookies = defaults_get_map(&self.auth_cookie_storage_key())?;
        let mut header = String::new();

        for (name, value) in cookies {
            let name = name.trim();
            let value = value.trim();
            if name.is_empty() || value.is_empty() {
                continue;
            }
            if !header.is_empty() {
                header.push_str("; ");
            }
            header.push_str(name);
            header.push('=');
            header.push_str(value);
        }

        if header.is_empty() {
            None
        } else {
            Some(header)
        }
    }

    fn clear_auth_cookies(&self, storage_key: &str) {
        defaults_set(&format!("{storage_key}.keys"), DefaultValue::Null);
        defaults_set(&format!("{storage_key}.values"), DefaultValue::Null);
    }

    fn validate_session(&self) -> Result<()> {
        let url = format!("{}/userdetail.php", self.base_url());
        let html = self.request_html(&url)?;
        let body_text = html
            .select_first("body")
            .and_then(|body| body.text())
            .unwrap_or_default();
        if body_text.contains("用户登录") || body_text.contains("用户名或邮箱") {
            bail!("Wenku8 未接受当前登录会话，请重新登录");
        }
        Ok(())
    }

    fn request_html(&self, url: &str) -> Result<Document> {
        let mut request = Request::get(url)?
            .header("User-Agent", USER_AGENT)
            .header("Referer", &self.base_url())
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.5");

        if let Some(cookie) = self.cookie_header() {
            request = request.header("Cookie", &cookie);
        }

        let response = request.send()?;
        self.validate_response_status(&response)?;

        let html = response.get_html()?;
        let body_text = html
            .select_first("body")
            .and_then(|body| body.text())
            .unwrap_or_default();
        let page_text = body_text.to_ascii_lowercase();

        if page_text.contains("本站正式关闭")
            || page_text.contains("本站已经关闭")
            || page_text.contains("site is closed")
        {
            bail!("Wenku8 站点已关闭：当前域名返回了站点关闭页面");
        }
        if page_text.contains("just a moment")
            || page_text.contains("checking your browser")
            || page_text.contains("sorry, you have been blocked")
            || page_text.contains("请完成安全验证")
            || page_text.contains("cloudflare")
        {
            bail!(
                "Wenku8 触发了 Cloudflare 安全验证：请在浏览器中确认站点可访问，或更换网络后重试"
            );
        }
        if html.select_first("form[name='frmlogin']").is_some()
            || body_text.contains("用户名或邮箱")
        {
            bail!("Wenku8 登录已失效：请在插件设置中重新登录当前站点");
        }
        Ok(html)
    }

    fn validate_response_status(&self, response: &Response) -> Result<()> {
        match response.status_code() {
            403 => bail!("Wenku8 返回 HTTP 403：访问被 Cloudflare 或站点安全策略拒绝"),
            401 => bail!("Wenku8 返回 HTTP 401：登录会话无效，请重新登录"),
            429 => bail!("Wenku8 返回 HTTP 429：请求过于频繁，请稍后再试"),
            status if status >= 500 => {
                bail!("Wenku8 服务器暂时不可用（HTTP {status}），请稍后重试");
            }
            _ => Ok(()),
        }
    }

    fn book_url(&self, key: &str) -> String {
        if key.starts_with("http://") || key.starts_with("https://") {
            key.to_string()
        } else {
            format!("{}/book/{key}.htm", self.base_url())
        }
    }

    fn reader_url(&self, key: &str) -> String {
        // Wenku8 的阅读目录入口长期使用 reader.php?aid=<id>
        format!("{}/modules/article/reader.php?aid={key}", self.base_url())
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
                        format!("{}{url}", self.base_url())
                    }),
                    ..Default::default()
                });
            }
        }

        entries
    }

    fn chapter_text(&self, chapter: &Chapter) -> Result<String> {
        let url = chapter.url.clone().unwrap_or_else(|| chapter.key.clone());

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
                "{}/modules/article/search.php?searchtype=articlename&searchkey={}&page={}&charset=utf-8",
                self.base_url(),
                encode_uri_component(query.trim()),
                page
            )
        } else {
            // 无搜索词时显示最近更新。
            format!(
                "{}/modules/article/toplist.php?sort=lastupdate&page={}",
                self.base_url(),
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
        let book_url = self.book_url(&manga.key);

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
            let html = self.request_html(&self.reader_url(&manga.key))?;
            let mut chapters: Vec<Chapter> = Vec::new();

            if let Some(links) = html.select("td.ccss a[href], .ccss a[href]") {
                for (index, link) in links.enumerate() {
                    let Some(href) = link.attr("abs:href").or_else(|| link.attr("href")) else {
                        continue;
                    };

                    let url = if href.starts_with("http://") || href.starts_with("https://") {
                        href
                    } else {
                        format!("{}/{href}", self.base_url())
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

impl WebLoginHandler for Wenku8 {
    fn handle_web_login(&self, key: String, cookies: HashMap<String, String>) -> Result<bool> {
        let storage_key = match key.as_str() {
            LOGIN_NET_KEY => format!("{AUTH_COOKIE_STORAGE_PREFIX}wenku8.net"),
            LOGIN_CC_KEY => format!("{AUTH_COOKIE_STORAGE_PREFIX}wenku8.cc"),
            _ => bail!("不支持的登录设置：{key}"),
        };

        let has_user_cookie = cookies
            .get(LOGIN_COOKIE_NAME)
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        let has_visit_cookie = cookies
            .get(VISIT_COOKIE_NAME)
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        let is_logged_in = has_user_cookie && has_visit_cookie;
        if is_logged_in {
            defaults_set(&storage_key, DefaultValue::HashMap(cookies));
            if let Err(error) = self.validate_session() {
                self.clear_auth_cookies(&storage_key);
                return Err(error);
            }
        } else {
            // Aidoku 在用户退出后会清除 WebView Cookie 并再次调用此处理器。
            self.clear_auth_cookies(&storage_key);
        }

        Ok(is_logged_in)
    }
}

register_source!(Wenku8, WebLoginHandler);

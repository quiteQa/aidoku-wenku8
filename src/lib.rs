#![no_std]

use aidoku::{
    alloc::{format, string::ToString, vec, String, Vec},
    imports::{
        defaults::{defaults_get, defaults_get_map, defaults_set, DefaultValue},
        html::{Document, Html},
        net::{set_rate_limit, Request, Response, TimeUnit},
    },
    prelude::*,
    Chapter, ContentRating, FilterValue, HashMap, Manga, MangaPageResult, MangaStatus, Page,
    ImageRequestProvider, PageContent, Result, Source, Viewer, WebLoginHandler,
};
use encoding_rs::GBK;

const DEFAULT_SITE: &str = "wenku8.net";
const SITE_SETTING_KEY: &str = "wenku8_site";
const LOGIN_NET_KEY: &str = "wenku8_login_net";
const LOGIN_CC_KEY: &str = "wenku8_login_cc";
const AUTH_COOKIE_STORAGE_PREFIX: &str = "wenku8_auth_cookies_";
const LOGIN_COOKIE_NAME: &str = "jieqiUserInfo";
const REQUEST_TIMEOUT_SECONDS: f64 = 20.0;

struct Wenku8;

impl Wenku8 {
    fn base_url(&self) -> String {
        // Wenku8 的页面与登录流程以 www 主机为准。直接请求裸域名可能
        // 触发额外重定向，且两个主机的 Cloudflare 策略可能不同。
        format!("https://www.{}", self.selected_site())
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
            // 验证 Cookie 由 Aidoku 的浏览器和网络会话管理，不能重放旧快照。
            if !name.starts_with("jieqi") {
                continue;
            }
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

    fn is_selected_site_url(&self, url: &str) -> bool {
        let site = self.selected_site();
        let origin = format!("https://{site}");
        let www_origin = format!("https://www.{site}");

        [origin, www_origin].iter().any(|origin| {
            let origin = origin.as_str();
            url == origin
                || url
                    .strip_prefix(origin)
                    .map(|rest| rest.starts_with('/'))
                    .unwrap_or(false)
        })
    }

    fn resolve_url(&self, href: &str, base_url: &str) -> Option<String> {
        let href = href.trim();
        if href.is_empty()
            || href.starts_with('#')
            || href.starts_with("javascript:")
            || href.starts_with("data:")
        {
            return None;
        }
        if href.starts_with("https://") || href.starts_with("http://") {
            return Some(href.to_string());
        }
        if href.starts_with("//") {
            return Some(format!("https:{href}"));
        }
        if href.starts_with('/') {
            return Some(format!("{}{href}", self.base_url()));
        }

        let base_without_fragment = base_url.split('#').next().unwrap_or(base_url);
        let base_without_query = base_without_fragment
            .split('?')
            .next()
            .unwrap_or(base_without_fragment);
        let directory = if let Some(authority_start) = base_without_query.find("://") {
            let path_start = authority_start + 3;
            if base_without_query[path_start..].contains('/') {
                base_without_query
                    .rsplit_once('/')
                    .map(|(directory, _)| directory)
                    .unwrap_or(base_without_query)
            } else {
                base_without_query
            }
        } else {
            base_without_query
                .rsplit_once('/')
                .map(|(directory, _)| directory)
                .unwrap_or(base_without_query)
        };
        Some(format!("{directory}/{href}"))
    }

    fn request_html(&self, url: &str) -> Result<Document> {
        self.send_html(Request::get(url)?, url)
    }

    fn send_html(&self, request: Request, url: &str) -> Result<Document> {
        let mut request = request
            .header("Referer", &self.base_url())
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.5")
            .timeout(REQUEST_TIMEOUT_SECONDS);

        // Cookie 必须只发送给它所属的 HTTPS 站点。章节 URL 会持久化，
        // 用户切换域名后可能仍打开旧域名章节，不能把新站点会话带过去。
        if self.is_selected_site_url(url) {
            if let Some(cookie) = self.cookie_header() {
                request = request.header("Cookie", &cookie);
            }
        }

        let response = request.send()?;
        self.validate_response_status(&response)?;

        // Wenku8 返回 GBK 字节。不要依赖客户端将旧编码识别为 UTF-8，
        // 否则可能得到空 DOM；与参考客户端一样，在解析前明确解码。
        let data = response.get_data()?;
        if data.is_empty() {
            bail!("Wenku8 返回了空响应，请稍后重试");
        }
        let decoded = match core::str::from_utf8(&data) {
            Ok(text) => text.to_string(),
            Err(_) => GBK.decode(&data).0.into_owned(),
        };
        let html = Html::parse_with_url(decoded.as_bytes(), url)?;
        if html.select_first("form[name='frmlogin'], form[action*='login.php'] input[type='password']").is_some() {
            bail!("Wenku8 返回登录页：请在插件设置中重新登录。若登录浏览器从 .cc 跳转到了 .net，请选择 .net 并登录该站点");
        }
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
        let status = response.status_code();
        if status == 403 {
            let is_challenge = response
                .get_header("cf-mitigated")
                .map(|value| value.to_ascii_lowercase().contains("challenge"))
                .unwrap_or(false);
            if is_challenge {
                bail!(
                    "Wenku8 触发了 Cloudflare 人机验证：请先用浏览器访问当前站点，或切换网络后重试"
                );
            }
            bail!(
                "Wenku8 返回 HTTP 403：当前 IP 或客户端被拒绝，请关闭代理或切换 Wi-Fi/蜂窝网络后重试"
            );
        }

        match status {
            401 => bail!("Wenku8 返回 HTTP 401：登录会话无效，请重新登录"),
            429 => {
                if let Some(retry_after) = response.get_header("Retry-After")
                    .and_then(|value| value.trim().parse::<u64>().ok()) {
                    bail!(
                        "Wenku8 返回 HTTP 429：请求过于频繁，请在 {retry_after} 秒后重试"
                    );
                }
                bail!("Wenku8 返回 HTTP 429：请求过于频繁，请稍后再试");
            }
            status if status >= 500 => {
                bail!("Wenku8 服务器暂时不可用（HTTP {status}），请稍后重试");
            }
            status if (200..300).contains(&status) => Ok(()),
            status => bail!("Wenku8 请求失败（HTTP {status}）"),
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

    fn cover_url(key: &str) -> String {
        let directory = if key.len() <= 3 {
            "0"
        } else {
            key.get(0..1).unwrap_or("0")
        };
        format!("https://img.wenku8.com/image/{directory}/{key}/{key}s.jpg")
    }

    fn encode_search_query(query: &str) -> String {
        // Wenku8 的简体搜索表单使用 GBK，而不是 UTF-8。
        // 按字节全部百分号编码，与站点表单及 hikari_novel_flutter 保持一致。
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let (encoded, _, _) = GBK.encode(query);
        let mut result = String::with_capacity(encoded.len() * 3);
        for byte in encoded.iter().copied() {
            result.push('%');
            result.push(HEX[(byte >> 4) as usize] as char);
            result.push(HEX[(byte & 0x0f) as usize] as char);
        }
        result
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
                        .replace("内容简介：", "")
                        .replace("内容简介", "")
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

        if let Some(links) = html.select("a[href*='/book/'], a[href*='articleinfo.php?id=']") {
            for link in links {
                let Some(url) = link.attr("abs:href")
                    .filter(|value| !value.trim().is_empty())
                    .or_else(|| link.attr("href")) else {
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
                    .filter(|value| !value.trim().is_empty())
                    .or_else(|| link.text())
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();

                if title.is_empty() {
                    continue;
                }

                seen.push(key.clone());
                let cover = Some(Self::cover_url(&key));
                entries.push(Manga {
                    key,
                    title,
                    cover,
                    url: self.resolve_url(&url, &self.base_url()),
                    ..Default::default()
                });
            }
        }

        entries
    }

    fn parse_single_search_result(&self, html: &Document) -> Option<Manga> {
        // 唯一匹配会直接跳到详情页；不要把详情页推荐栏误当搜索结果。
        let link = html.select_first("#content a[href*='addbookcase.php?bid=']")?;
        let href = link.attr("href")?;
        let key = href.split("bid=").nth(1)?.chars()
            .take_while(|c| c.is_ascii_digit()).collect::<String>();
        if key.is_empty() { return None; }
        let title = Self::first_text(html, &["#content table b", "#content h1"])?;
        Some(Manga {
            cover: Some(Self::cover_url(&key)),
            url: Some(self.book_url(&key)),
            key,
            title,
            ..Default::default()
        })
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
        // 由 Aidoku 在网络层统一排队，比在每个入口手动 sleep 更可靠；
        // 既抑制首页、搜索和详情同时刷新产生的突发请求，也不会阻塞解析逻辑。
        set_rate_limit(4, 1, TimeUnit::Seconds);
        Self
    }

    fn get_search_manga_list(
        &self,
        query: Option<String>,
        page: i32,
        _filters: Vec<FilterValue>,
    ) -> Result<MangaPageResult> {
        let page = page.max(1);
        let html = if let Some(query) = query.filter(|q| !q.trim().is_empty()) {
            let encoded = Self::encode_search_query(query.trim());
            if page == 1 {
                // 与登录后实际网页表单一致：GBK 表单 POST 到 so.php。
                let url = format!("{}/so.php", self.base_url());
                let body = format!("searchtype=articlename&searchkey={encoded}&charset=&Submit=%C7%E1%D0%A1%CB%B5%CB%D1%CB%F7");
                self.send_html(Request::post(&url)?
                    .header("Content-Type", "application/x-www-form-urlencoded")
                    .body(body.as_bytes()), &url)?
            } else {
                // 翻页沿用搜索结果页提供的 GET 参数，不添加额外 charset 参数。
                let url = format!("{}/modules/article/search.php?searchtype=articlename&searchkey={encoded}&page={page}", self.base_url());
                self.request_html(&url)?
            }
        } else {
            // 无搜索词时显示最近更新。
            let url = format!(
                "{}/modules/article/toplist.php?sort=lastupdate&page={}",
                self.base_url(),
                page
            );
            self.request_html(&url)?
        };

        if let Some(manga) = self.parse_single_search_result(&html) {
            return Ok(MangaPageResult { entries: vec![manga], has_next_page: false });
        }
        let entries = self.parse_search_results(&html);

        if entries.is_empty() {
            let site_error = Self::first_text(&html, &[".blocktitle"])
                .map(|title| title.contains("出现错误") || title.contains("出現錯誤"))
                .unwrap_or(false);
            if site_error {
                bail!("Wenku8 返回站点错误页面：可能是搜索过快或需要登录，请打开当前站点确认后重试");
            }
            // 空列表不能被当成成功，否则 Aidoku 只显示白屏。
            let title = Self::first_text(&html, &["title"])
                .unwrap_or_else(|| "无标题".to_string())
                .chars().take(80).collect::<String>();
            let links = html.select("a[href]").map(|links| links.count()).unwrap_or(0);
            bail!("Wenku8 未解析到书籍（页面：{title}，链接数：{links}）。请打开当前站点确认登录状态；若网页正常，请反馈此提示");
        }

        // 只有页面确实链接到下一页时才继续，避免最后一页重复加载。
        let next_page_marker = format!("page={}", page.saturating_add(1));
        let has_next_page = html
            .select("a[href]")
            .map(|mut links| {
                links.any(|link| {
                    link.attr("href")
                        .map(|href| href.split('?').nth(1)
                            .map(|query| query.split('#').next().unwrap_or(query)
                                .split('&').any(|part| part == next_page_marker))
                            .unwrap_or(false))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);

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

            let parsed_cover = Self::first_attr(
                &html,
                &["img[vspace]", "#content img", "table img"],
                "abs:src",
            )
            .or_else(|| {
                Self::first_attr(&html, &["img[vspace]", "#content img", "table img"], "src")
            })
            .and_then(|url| self.resolve_url(&url, &book_url));

            // 详情页偶尔会省略封面或返回相对路径，不能覆盖列表页已经可用的封面。
            manga.cover = parsed_cover
                .or(manga.cover)
                .or_else(|| Some(Self::cover_url(&manga.key)));

            manga.authors = Self::parse_author(&html).map(|a| vec![a]);
            manga.description = Self::parse_description(&html);
            manga.url = Some(book_url.clone());
            manga.content_rating = ContentRating::Safe;
            manga.status = MangaStatus::Unknown;
            manga.viewer = Viewer::RightToLeft;
        }

        if needs_chapters {
            let reader_url = self.reader_url(&manga.key);
            let html = self.request_html(&reader_url)?;
            let mut chapters: Vec<Chapter> = Vec::new();

            if let Some(links) = html.select("td.ccss a[href], .ccss a[href]") {
                for (index, link) in links.enumerate() {
                    let Some(href) = link.attr("abs:href")
                        .filter(|value| !value.trim().is_empty())
                        .or_else(|| link.attr("href")) else {
                        continue;
                    };

                    let Some(url) = self.resolve_url(&href, &reader_url) else {
                        continue;
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

impl ImageRequestProvider for Wenku8 {
    fn get_image_request(
        &self,
        url: String,
        _context: Option<aidoku::PageContext>,
    ) -> Result<Request> {
        // 不覆盖 User-Agent，让 Aidoku 使用与其 WebView 一致的默认标识。
        Ok(Request::get(&url)?
            .header("Referer", &self.base_url())
            .header("Accept", "image/avif,image/webp,image/apng,image/*,*/*;q=0.8")
            .timeout(REQUEST_TIMEOUT_SECONDS))
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
        // Aidoku 的回调不保证每个站点 Cookie 都会被返回；
        // jieqiVisitInfo 可能因域、Path 或 WebView Cookie 策略暂时缺失。
        // 只要核心登录 Cookie 存在，就先保存完整 Cookie Map，避免登录状态被误判为失败。
        let is_logged_in = has_user_cookie;
        if is_logged_in {
            defaults_set(&storage_key, DefaultValue::HashMap(cookies));
        } else {
            // Aidoku 在用户退出后会清除 WebView Cookie 并再次调用此处理器。
            self.clear_auth_cookies(&storage_key);
        }

        Ok(is_logged_in)
    }
}

register_source!(Wenku8, WebLoginHandler, ImageRequestProvider);

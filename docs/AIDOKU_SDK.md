# Aidoku 开发文档与 SDK 对照

本源以 [Aidoku 源开发文档](https://aidoku.github.io/aidoku-rs/book/) 和
[Aidoku Rust SDK](https://github.com/Aidoku/aidoku-rs) 为接口依据。
2026-10-25 核对资源配置、Source traits、网络 API、登录回调及客户端深链接入口。

## 固定版本

`Cargo.toml` 显式固定 SDK 提交 `e1320b0a2e11afb59e4dee374883a2212d325699`，
`Cargo.lock` 使用同一 Git 来源。CI 从 `Cargo.toml` 读取 `rev` 来安装 Aidoku CLI，
让编译 SDK、打包工具和资源 Schema 对应同一版本。调整 SDK 时应同时更新锁文件，
运行完整构建并进行 Aidoku 真机验证。

## 接口对应

| 官方接口／配置 | 本源实现与验证 |
| --- | --- |
| `Source`、`register_source!` | 初始化、搜索、详情／目录、章节均通过 SDK traits 实现；WASM 测试检查实际导出。 |
| `info.urls` | 多域名仅声明 `urls`，按文档替代 `url`，避免客户端重复收集主域名。 |
| `config.maximumParallelRequests` | 固定为 5，是客户端最大并发配置；不随每秒请求数的开关变化。 |
| `net::set_rate_limit` | 设置请求频率，默认每秒 5 次；0 取消该额外限额。服务端 Retry-After 冷却独立保留。 |
| `helpers::uri::encode_uri_component` | 搜索词先转成 Wenku8 所需 GBK 字节，再调用 SDK 编码，防止空格、`&`、`+` 和 `%` 改变表单结构。 |
| `ImageRequestProvider` | 返回图片请求及对应 Referer；不附带源登录 Cookie。 |
| `WebLoginHandler`、`defaults` | 复用浏览器产生的站点 Cookie；登录标记不等同于服务端已验证会话。 |
| `DeepLinkHandler` | 只解析已声明 Wenku8 域名与有效路径，返回小说／章节 key。 |
| `PageContent::Text` | 正文通过文本页返回；插图使用 URL 页。 |
| `aidoku package`、`aidoku verify` | 单源打包与官方 Schema／导出校验；统一由 `build.sh` 执行。 |

SDK 的限速接口和客户端的最大并发配置是两个独立机制。当前官方接口没有在源设置中
动态调整 `maximumParallelRequests` 的功能，所以设置名称保留为“每秒请求数”。
客户端原生图片请求走独立下载通道，书源网络限速不涵盖该通道。

## 深链接的实际入口

官方文档允许把网页链接的 `https` 替换成 `aidoku`，例如：

```text
aidoku://www.wenku8.net/book/3057.htm
aidoku://wenku8.cc/novel/3/3057/176892.htm
```

Aidoku 的 `AppDelegate.handleDeepLink` 依据源 URL 匹配入口，再将协议恢复为 HTTPS，
然后调用书源的 `DeepLinkHandler`。本源接收这一步产生的 HTTP(S) URL；WASM 测试模拟
客户端的协议转换和域名分发，并检查伪装域名仍被拒绝。

## 验证范围

`sh build.sh` 依次执行 Rust 策略／URL 测试、release WASM 编译、WASM 宿主集成测试、
官方打包与校验。WASM 测试同时检查 trait 导出、宿主导入和 SDK／CLI 版本约束。
VS Code 的资源配置提示也指向固定 SDK 的官方 Schema。

HTTP、时钟、设置和固定 DOM 均为测试模拟，不覆盖 SwiftSoup 的实际解析行为；
登录验证、网站限流、原生图片下载和设备阅读仍需真机验收。

## 官方依据

- [资源文件配置](https://github.com/Aidoku/aidoku-rs/blob/main/book/src/ch02-01-resource-files.md)
- [Rust 程序、trait 注册、网络与 HTML API](https://github.com/Aidoku/aidoku-rs/blob/main/book/src/ch02-02-the-rust-program.md)
- [打包后的运行方式](https://github.com/Aidoku/aidoku-rs/blob/main/book/src/ch03-00-running.md)
- [固定 SDK 的网络 API](https://github.com/Aidoku/aidoku-rs/blob/e1320b0a2e11afb59e4dee374883a2212d325699/crates/lib/src/imports/net.rs)
- [固定 SDK 的 URI 工具](https://github.com/Aidoku/aidoku-rs/blob/e1320b0a2e11afb59e4dee374883a2212d325699/crates/lib/src/helpers/uri.rs)
- [固定 SDK 的 Source traits](https://github.com/Aidoku/aidoku-rs/blob/e1320b0a2e11afb59e4dee374883a2212d325699/crates/lib/src/structs/source.rs)
- [客户端深链接入口](https://github.com/Aidoku/Aidoku/blob/main/Aidoku/App/AppDelegate.swift)
- [客户端限速实现](https://github.com/Aidoku/AidokuRunner/blob/2ed3f0d395be0c3ee9ebe238e2ffcead0094613f/Sources/AidokuRunner/Utilities/RateLimit.swift)

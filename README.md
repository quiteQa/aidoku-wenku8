# Aidoku Wenku8 中文轻小说源（MVP）

目标站点：`https://www.wenku8.net/`

这是一个基于 Aidoku 当前 Rust Source API 的 Wenku8 文本小说源原型，章节正文使用：

```rust
PageContent::text(...)
```

因此章节会进入 Aidoku 的 **Text Reader**，不是旧式“文字渲染成图片”。

## 已实现

- 搜索轻小说
- 无搜索词时读取最近更新
- 小说详情页
- 封面（多选择器 fallback）
- 作者与简介（尽力解析）
- reader.php 章节目录
- `#acontent` / `#content` 正文解析
- Aidoku Text Reader
- 普通浏览器 User-Agent / Referer 请求头

## 重要限制

Wenku8 当前会对部分服务器/IP 返回 403，并且部分页面可能要求登录。

本项目**没有实现绕过登录、验证码或反爬机制**。如果你在 iPhone 上测试时：
- 搜索能打开，但目录报错；
- 或直接出现 403 / 登录页；

那么下一步应在 Aidoku 源中增加**正常登录后的 Cookie 共享/设置支持**，而不是绕过站点访问控制。

另外，由于当前外部测试环境访问 Wenku8 会得到 403，我无法在这里对真实 HTML 做最终实机校验。代码因此使用了多个兼容选择器，第一次安装后最好测试 1～2 本小说。

## 编译

安装 Rust、WebAssembly 目标和 Aidoku 官方命令行工具：

```bash
rustup target add wasm32-unknown-unknown
cargo install --git https://github.com/Aidoku/aidoku-rs aidoku-cli
```

在本目录运行以下命令生成可安装的 `package.aix`：

```bash
aidoku package .
```

也可以直接执行仓库内置脚本；它会打包并进行发布前校验：

```bash
./build.sh
```

当前 `aidoku build` 用于构建多个已打包源组成的来源列表，并不用于打包单个源项目。

## 推荐测试

安装后依次验证：

1. 搜索一个确定存在的作品名；
2. 打开作品详情；
3. 查看是否能加载目录；
4. 打开任意章节；
5. 确认进入 Aidoku Text Reader；
6. 检查段落换行、中文标点、插图缺失情况。

### 已知待完善

- 登录/Cookie 支持
- 更准确的“连载中 / 已完结”识别
- 分类/标签
- 作品详情页简介的精确 selector
- 正文 `<br>` 段落保留优化
- 章节内插图
- Wenku8 的繁简/编码兼容
- 更可靠的分页判断

如果测试后把 Aidoku 的报错文字或页面截图发回来，就可以继续针对实际页面把 selector 修到稳定版。

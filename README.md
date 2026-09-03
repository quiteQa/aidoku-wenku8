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
- Aidoku 内嵌 WebView 的正常网页登录
- 可选择 `wenku8.net` 或 `wenku8.cc`
- 登录后的 Wenku8 `jieqiUserInfo` Cookie 会话复用
- 两个站点分别保存 Cookie，切换站点后需要登录对应站点
- 在 Aidoku 设置页注销并清除该源的 WebView Cookie

## 重要限制

Wenku8 当前会对部分服务器/IP 返回 403，并且部分页面可能要求登录。

本项目**不会绕过登录、验证码或反爬机制**。当 Wenku8 要求登录时，请在 Aidoku 中打开此源的设置，选择“登录 Wenku8”，并在官方页面内自行完成登录。源只使用该网页登录产生的 Cookie 来请求受限页面，**不会读取、保存或提交你的账号密码**。

登录完成后，Aidoku 会把所选站点的合法会话用于该站点后续请求。若切换到另一个站点，需要在设置中打开对应的登录入口重新登录；两个站点的 Cookie 会分别保存。如需清除会话，请在同一设置页选择对应站点的退出操作。由于 Wenku8 可能返回 403 或站点关闭页面，首次安装后建议分别测试两个站点的搜索、作品目录和任意章节。

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

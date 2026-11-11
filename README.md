# yd — 有道翻译

[![CI](https://github.com/totrytakeoff/yd-translate/actions/workflows/ci.yml/badge.svg)](https://github.com/totrytakeoff/yd-translate/actions/workflows/ci.yml)
[![Release](https://github.com/totrytakeoff/yd-translate/actions/workflows/release.yml/badge.svg)](https://github.com/totrytakeoff/yd-translate/actions/workflows/release.yml)

一个后端、两个前端：

```
有道官方 API（整句） + 有道词典接口（词条）
                │
          yd-core（Rust）
           ┌────┴─────┐
        yd（CLI）   yd-gui（Tauri 浮动窗）
           │
      desk-run translate
           │
   quickshell launcher「工具」页
```

## 组成

| 目录 | 说明 |
|---|---|
| `yd-core/` | 签名、翻译、词典解析、缓存（纯 Rust，可跨平台） |
| `yd-cli/` | 命令行 `yd`，输出人类可读文本或单行 JSON |
| `yd-gui/src-tauri/` | Tauri 2 浮动窗，关闭即退出 |
| `yd-gui/ui/` | 原生 HTML/CSS/JS，无框架 |

## 安装

**Arch / AUR**

```bash
yay -S yd-translate-bin
```

**Debian / Ubuntu**

```bash
sudo dpkg -i yd-translate_<version>_amd64.deb
```

预编译包（Linux tar.gz / Windows zip / macOS tar.gz）见
[Releases](https://github.com/totrytakeoff/yd-translate/releases)。

**从源码**

```bash
git clone https://github.com/totrytakeoff/yd-translate.git
cd yd-translate
cargo build --release
install -Dm755 target/release/yd      ~/.local/bin/yd
install -Dm755 target/release/yd-gui  ~/.local/bin/yd-gui
```

## 配置

```bash
yd --init-config     # 生成 ~/.config/yd/conf（0600）
yd --check           # 校验 appKey/appSecret
```

配置文件内容（有道智云控制台 → 应用管理）：

```
app_key = 应用ID
app_secret = 应用密钥
```

也可以用环境变量 `YOUDAO_APP_KEY` / `YOUDAO_APP_SECRET` 覆盖。
注意：控制台的「API Key」不参与 v3 签名，填不填都不影响。

## 用法

```bash
yd hello              # 查词：音标 / 词性释义 / 词形 / 例句 / 网络释义
yd 今天天气不错        # 整句翻译
yd -t en 你好          # 指定目标语言
yd -b hello           # 只要一行
yd -j hello           # 单行 JSON（给 UI 用）
yd -p hello           # 朗读
yd -i                 # 交互模式
echo "text" | yd      # 管道

# OCR 识别图片文字后翻译（有道 OCR API，跨平台）
yd --ocr screenshot.png                  # 默认按中文识别
yd --ocr screenshot.png --ocr-lang en    # 英文图片
grim -g "$(slurp)" - | yd --ocr - --ocr-lang zh-CHS   # Linux 截图管道
```

没有 appKey 时：
- 单词仍可查（免 key 词典接口，音标/释义/例句齐全）
- 整句先走 Google 网页接口（`clients5.google.com`，约 1–2 秒）；
  失败再回落到 Bing（需要系统里有 `trans`）

凭证也可以在 GUI 里填：点右上角「设置」→ 应用ID / 应用密钥 → 保存并校验。
配置文件权限为 0600。

> 以 `-` 开头的文本请用 `--` 分隔，例如 `yd -- -5`；未知参数会直接报错，
> 不会被当成待翻译文本。

## 桌面集成

- **GUI 行为**：Esc 手动关闭（失焦不关）；空输入时显示最近记录；
  重复启动会唤到已有窗口；底部有朗读/复制按钮。
- **GUI 图片翻译**：点右上角「OCR」进入图片翻译界面，支持四种输入：
  1. `Ctrl+V`：keydown 捕获后调原生剪贴板读取（WebKitGTK 的 paste 事件拿不到图片）
  2. 拖入图片文件：Tauri 窗口级 `DragDropEvent` 在 Rust 侧接收，再 `eval` 调前端
  3. 点虚线框打开系统文件选择器
  4. 点「从剪贴板粘贴」按钮（arboard 原生读取，只在用户操作时读）
  拖放不限于 OCR 界面，主界面拖入也会直接识别。
- **应用菜单**：`~/.local/share/applications/yd-gui.desktop`
  （名称 `yd 有道翻译`，在 quickshell launcher 里搜 `yd` 即可启动）
- **quickshell**：`Modules/Launcher/ToolsPage.qml` 调用
  `desk-run translate --json -s en -t zh-CHS -- <text>`，
  结果由 `TranslateResult.qml` 渲染。纯 QML，无 C++ 胶水。
- **desk-run**：新增子命令
  `translate) exec "${TOOL_DIR}/yd" "$@" ;;`
  （`~/.config/my-desk/bin/yd` 软链到 `~/.local/bin/yd`）
- **niri**：
  - 热键 `Alt+Shift+T` 启动 `yd-gui`
  - 窗口规则 `match app-id=r#"^yd-gui$"#` + `open-floating true`

## 缓存

`~/.cache/yd/cache.json`，7 天过期，最多 500 条，`-c/--no-cache` 可跳过。

## 已知限制

- 词典数据来自 `dict.youdao.com/jsonapi`（非官方），官方 v3 API 已不下发词典数据；接口变更时需要修 `yd-core/src/dict.rs`。
- 有道官方翻译 API 的 `basic` 字段已下线，音标/释义只能走上面那个接口。
- OCR 用的是 `https://openapi.youdao.com/ocrapi`（`detectType=10012` 按行识别），
  与应用里是否开通「通用文字识别」有关；`langType` 用 `zh-CHS` / `en` 等。
- Wayland 下应用不能自行置顶/定位，浮动与位置由 niri 的 window-rule 决定。
- GUI 冷启动约 0.3–0.5 秒；关闭（Esc / 失焦）会结束进程，不留常驻。

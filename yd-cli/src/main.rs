//! yd —— 有道翻译 / 词典命令行工具

use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::process::Command;

use clap::Parser;

use yd_core::api;
use yd_core::config::Config;
use yd_core::model::{DictInfo, QueryResult};
use yd_core::{detect_lang, query, tts_url, Mode, QueryOptions};

#[derive(Parser, Debug)]
#[command(
    name = "yd",
    version,
    about = "有道翻译 / 词典命令行工具",
    long_about = "官方文本翻译 API + 免 key 词典接口，双通道自动选择。",
    after_help = "示例:\n  yd hello\n  yd 今天天气不错\n  yd -t en 你好\n  echo text | yd\n  yd -j hello"
)]
struct Cli {
    /// 要查询或翻译的文本；省略则读 stdin
    #[arg(value_name = "TEXT", trailing_var_arg = true)]
    text: Vec<String>,

    /// 源语言（默认 auto）
    #[arg(short = 's', long = "from", default_value = "auto")]
    from: String,

    /// 目标语言（默认 zh-CHS）
    #[arg(short = 't', long = "to", default_value = "zh-CHS")]
    to: String,

    /// 精简输出，只要一行
    #[arg(short = 'b', long = "brief")]
    brief: bool,

    /// 强制词典模式
    #[arg(short = 'd', long = "dict", conflicts_with = "translate")]
    dict: bool,

    /// 强制翻译模式
    #[arg(short = 'x', long = "translate")]
    translate: bool,

    /// 输出单行 JSON
    #[arg(short = 'j', long = "json")]
    json: bool,

    /// 朗读
    #[arg(short = 'p', long = "play")]
    play: bool,

    /// 交互模式
    #[arg(short = 'i', long = "interactive")]
    interactive: bool,

    /// 禁用缓存
    #[arg(short = 'c', long = "no-cache")]
    no_cache: bool,

    /// 检查 appKey 是否可用
    #[arg(long = "check")]
    check: bool,

    /// 生成配置文件模板
    #[arg(long = "init-config")]
    init_config: bool,

    /// 列出语言代码
    #[arg(short = 'l', long = "langs")]
    langs: bool,

    /// 识别图片文字并翻译（PATH 为图片路径，- 表示 stdin）
    #[arg(long = "ocr", value_name = "PATH")]
    ocr: Option<String>,

    /// OCR 识别语言（默认 zh-CHS）
    #[arg(long = "ocr-lang", value_name = "LANG", default_value = "zh-CHS")]
    ocr_lang: String,

    /// 不使用颜色
    #[arg(long = "no-color")]
    no_color: bool,
}

impl Cli {
    fn options(&self, from_override: Option<String>) -> QueryOptions {
        QueryOptions {
            from: Some(from_override.unwrap_or_else(|| self.from.clone())),
            to: Some(self.to.clone()),
            mode: if self.dict {
                Some(Mode::Dict)
            } else if self.translate {
                Some(Mode::Translate)
            } else {
                None
            },
            no_cache: self.no_cache,
            fallback_bing: true,
        }
    }

    fn style(&self) -> Style {
        let enabled =
            !self.no_color && std::env::var_os("NO_COLOR").is_none() && io::stdout().is_terminal();
        Style { on: enabled }
    }
}

struct Style {
    on: bool,
}

impl Style {
    fn paint(&self, code: &str, text: &str) -> String {
        if self.on {
            format!("\x1b[{}m{}\x1b[0m", code, text)
        } else {
            text.to_string()
        }
    }
    fn bold(&self, text: &str) -> String {
        self.paint("1", text)
    }
    fn dim(&self, text: &str) -> String {
        self.paint("2", text)
    }
    fn green(&self, text: &str) -> String {
        self.paint("32", text)
    }
    fn yellow(&self, text: &str) -> String {
        self.paint("33", text)
    }
    fn red(&self, text: &str) -> String {
        self.paint("31", text)
    }
}

fn print_dict(style: &Style, info: &DictInfo) {
    println!("{}", style.bold(&info.word));

    let mut phonetic = String::new();
    if !info.uk.is_empty() {
        phonetic.push_str(&format!("英[{}] ", info.uk));
    }
    if !info.us.is_empty() {
        phonetic.push_str(&format!("美[{}]", info.us));
    }
    if !phonetic.trim().is_empty() {
        println!("{}", style.dim(phonetic.trim_end()));
    }

    for entry in &info.entries {
        println!("  {}", entry);
    }
    if !info.forms.is_empty() {
        let forms: Vec<String> = info
            .forms
            .iter()
            .map(|f| format!("{} {}", f.name, f.value))
            .collect();
        println!("{}", style.dim(&format!("  词形: {}", forms.join("；"))));
    }
    if !info.examples.is_empty() {
        println!("{}", style.dim("  例句:"));
        for example in &info.examples {
            println!("{}", style.dim(&format!("    {}", example.src)));
            if !example.dst.is_empty() {
                println!("{}", style.dim(&format!("    {}", example.dst)));
            }
        }
    }
    if !info.web.is_empty() {
        println!("{}", style.dim("  网络释义:"));
        for line in &info.web {
            println!("{}", style.dim(&format!("    · {}", line)));
        }
    }
}

fn emit(cli: &Cli, result: &QueryResult) {
    let style = cli.style();

    if cli.json {
        match serde_json::to_string(result) {
            Ok(json) => println!("{}", json),
            Err(err) => eprintln!("{}", style.red(&format!("[错误] JSON 序列化失败: {err}"))),
        }
        return;
    }

    if !result.ok {
        if let Some(error) = &result.error {
            eprintln!("{}", style.red(&format!("[错误] {}", error.message)));
            if error.code == "108" || error.code == "202" || error.code == "CONFIG" {
                eprintln!(
                    "{}",
                    style.dim(&format!(
                        "  配置文件: {}（运行 `yd --init-config` 生成，`yd --check` 校验）",
                        Config::config_path().display()
                    ))
                );
            }
        }
        return;
    }

    if let Some(info) = &result.dict {
        if cli.brief {
            if let Some(first) = info.entries.first() {
                println!("{}", first);
            } else if let Some(first) = info.web.first() {
                println!("{}", first);
            }
        } else {
            print_dict(&style, info);
        }
        return;
    }

    if cli.brief {
        println!("{}", result.translation);
    } else {
        println!("{}", style.dim(&result.query));
        println!("{}", style.green(&result.translation));
        if result.source == "google" || result.source == "bing" {
            let provider = if result.source == "google" {
                "Google 网页"
            } else {
                "Bing"
            };
            eprintln!(
                "{}",
                style.yellow(&format!(
                    "[提示] 未配置有道 appKey，本次结果来自 {provider}"
                ))
            );
        }
    }
}

fn play(result: &QueryResult) {
    let text = if result.mode == "dict" {
        result
            .dict
            .as_ref()
            .map(|d| d.word.clone())
            .unwrap_or_else(|| result.query.clone())
    } else {
        result.query.clone()
    };
    let lang = if result.from.is_empty() {
        detect_lang(&text).to_string()
    } else {
        result.from.clone()
    };
    let url = tts_url(&text, &lang);

    let player = ["mpv", "ffplay", "play"]
        .iter()
        .find(|name| which(name).is_some());
    let Some(player) = player else {
        eprintln!("[提示] 没有找到 mpv / ffplay，无法朗读");
        return;
    };

    let mut command = Command::new(player);
    match *player {
        "mpv" => {
            command.args(["--no-config", "--really-quiet"]);
        }
        "ffplay" => {
            command.args(["-nodisp", "-autoexit", "-loglevel", "quiet"]);
        }
        _ => {}
    }
    let _ = command.arg(&url).spawn();
}

fn which(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// 读取待识别图片：文件路径或 stdin（-）。
fn read_image(path: &str) -> Result<Vec<u8>, String> {
    if path == "-" {
        let mut buffer = Vec::new();
        io::stdin()
            .read_to_end(&mut buffer)
            .map_err(|e| format!("读取 stdin 失败：{e}"))?;
        Ok(buffer)
    } else {
        std::fs::read(path).map_err(|e| format!("读取图片失败：{e}"))
    }
}

fn run_query(cli: &Cli, text: &str, from_override: Option<String>) -> QueryResult {
    let result = query(text, &cli.options(from_override));
    emit(cli, &result);
    result
}

fn interactive(cli: &Cli) {
    println!("交互模式，输入 :q 退出，:h 查看帮助");
    let stdin = io::stdin();
    loop {
        print!("yd> ");
        let _ = io::stdout().flush();
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match line {
            ":q" | ":quit" | ":exit" | "exit" | "quit" => break,
            ":h" | ":help" | "?" => {
                println!("  -t 目标语言  -s 源语言  -p 朗读  -b 精简  :q 退出");
                continue;
            }
            _ => {}
        }
        run_query(cli, line, None);
    }
}

fn list_langs() {
    let langs = [
        ("auto", "自动检测"),
        ("zh-CHS", "简体中文"),
        ("zh-CHT", "繁体中文"),
        ("en", "英语"),
        ("ja", "日语"),
        ("ko", "韩语"),
        ("fr", "法语"),
        ("es", "西班牙语"),
        ("pt", "葡萄牙语"),
        ("it", "意大利语"),
        ("ru", "俄语"),
        ("de", "德语"),
        ("ar", "阿拉伯语"),
        ("th", "泰语"),
        ("vi", "越南语"),
        ("id", "印尼语"),
        ("nl", "荷兰语"),
        ("tr", "土耳其语"),
    ];
    for (code, name) in langs {
        println!("{:<8} {}", code, name);
    }
}

#[cfg(unix)]
fn reset_sigpipe() {
    // 管道下游提前关闭（如 | head）时安静退出，而不是 panic。
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe() {}

fn main() {
    reset_sigpipe();
    let cli = Cli::parse();
    let style = cli.style();

    if cli.init_config {
        match Config::init_file() {
            Ok(path) => {
                println!("配置文件: {}", path.display());
                println!("填入 app_key / app_secret 后运行 `yd --check` 校验");
            }
            Err(err) => {
                eprintln!("{}", style.red(&format!("[错误] 创建配置失败: {err}")));
                std::process::exit(1);
            }
        }
        return;
    }

    if cli.check {
        let cfg = Config::load();
        match api::check(&cfg) {
            Ok(text) => {
                println!("{} appKey 可用", style.green("✓"));
                println!("测试翻译: hello -> {}", text);
            }
            Err(err) => {
                eprintln!("{}", style.red(&format!("[错误] {}", err)));
                std::process::exit(1);
            }
        }
        return;
    }

    if cli.langs {
        list_langs();
        return;
    }

    if cli.interactive {
        interactive(&cli);
        return;
    }

    let mut text = cli.text.join(" ");
    let mut from_override = None;
    if let Some(path) = cli.ocr.clone() {
        let recognized = read_image(&path).and_then(|bytes| {
            let cfg = Config::load();
            yd_core::ocr::recognize(&bytes, &cli.ocr_lang, &cfg).map_err(|e| e.to_string())
        });
        match recognized {
            Ok(recognized) => {
                eprintln!(
                    "{}",
                    style.dim(&format!(
                        "[OCR] 识别到 {} 个字符",
                        recognized.chars().count()
                    ))
                );
                text = recognized;
                from_override = Some(cli.ocr_lang.clone());
            }
            Err(message) => {
                eprintln!("{}", style.red(&format!("[错误] {message}")));
                std::process::exit(1);
            }
        }
    }
    if text.trim().is_empty() {
        if io::stdin().is_terminal() {
            let mut help = <Cli as clap::CommandFactory>::command();
            let _ = help.print_help();
            return;
        }
        let mut buffer = String::new();
        if io::stdin().read_to_string(&mut buffer).is_ok() {
            text = buffer;
        }
    }

    if text.trim().is_empty() {
        eprintln!("{}", style.red("[错误] 没有输入文本"));
        std::process::exit(1);
    }

    let result = run_query(&cli, text.trim(), from_override);
    if cli.play && result.ok {
        play(&result);
    }
    std::process::exit(if result.ok { 0 } else { 1 });
}

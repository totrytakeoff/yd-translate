//! 配置加载：~/.config/yd/conf，环境变量优先。

use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub app_key: String,
    pub app_secret: String,
}

impl Config {
    pub fn config_path() -> PathBuf {
        let base = env::var("XDG_CONFIG_HOME")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".config"));
        base.join("yd").join("conf")
    }

    pub fn cache_path() -> PathBuf {
        let base = env::var("XDG_CACHE_HOME")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".cache"));
        base.join("yd").join("cache.json")
    }

    pub fn load() -> Config {
        let mut cfg = Config::default();
        if let Ok(text) = fs::read_to_string(Self::config_path()) {
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let (key, value) = match line.find(['=', ':']) {
                    Some(i) => (&line[..i], &line[i + 1..]),
                    None => continue,
                };
                let key = key.trim().to_ascii_lowercase();
                let value = value
                    .trim()
                    .trim_matches(|c| c == '"' || c == '\'')
                    .to_string();
                match key.as_str() {
                    "app_key" | "appkey" => cfg.app_key = value,
                    "app_secret" | "appsecret" | "secret" => cfg.app_secret = value,
                    _ => {}
                }
            }
        }
        if let Ok(v) = env::var("YOUDAO_APP_KEY") {
            if !v.is_empty() {
                cfg.app_key = v;
            }
        }
        if let Ok(v) = env::var("YOUDAO_APP_SECRET") {
            if !v.is_empty() {
                cfg.app_secret = v;
            }
        }
        cfg
    }

    pub fn has_credentials(&self) -> bool {
        !self.app_key.is_empty() && !self.app_secret.is_empty()
    }

    /// 保存凭证（原子写入，0600），返回配置文件路径。
    pub fn save(app_key: &str, app_secret: &str) -> std::io::Result<PathBuf> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = format!(
            "# 有道智云 https://ai.youdao.com -> 控制台 -> 应用管理\n\
             # 应用ID（16 位）与应用密钥（32 位）\n\
             app_key = {}\n\
             app_secret = {}\n",
            app_key.trim(),
            app_secret.trim()
        );
        let tmp = path.with_extension("conf.tmp");
        fs::write(&tmp, content)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
        }
        fs::rename(&tmp, &path)?;
        Ok(path)
    }

    /// 写入模板配置（0600），返回配置文件路径。
    pub fn init_file() -> std::io::Result<PathBuf> {
        let path = Self::config_path();
        if path.exists() {
            return Ok(path);
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(
            &path,
            "# 有道智云 https://ai.youdao.com -> 控制台 -> 应用管理\n\
             # 应用ID（16 位）与应用密钥（32 位）\n\
             app_key = \n\
             app_secret = \n",
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
        Ok(path)
    }
}

pub fn home() -> PathBuf {
    env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

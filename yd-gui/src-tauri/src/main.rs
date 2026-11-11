//! yd-gui：有道翻译浮动窗。
//! 手动 Esc 关闭；不再因失焦退出。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;
use yd_core::{api, cache, config::Config, query, QueryOptions};

#[tauri::command]
async fn translate(text: String, from: String, to: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let opts = QueryOptions {
            from: Some(from),
            to: Some(to),
            mode: None,
            no_cache: false,
            fallback_bing: true,
        };
        serde_json::to_value(query(&text, &opts)).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
async fn ocr_translate(
    image: String,
    lang: String,
    to: String,
) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = Config::load();
        let text =
            yd_core::ocr::recognize_data_url(&image, &lang, &cfg).map_err(|e| e.to_string())?;
        let opts = QueryOptions {
            from: Some(lang),
            to: Some(to),
            mode: None,
            no_cache: false,
            fallback_bing: true,
        };
        let result = query(&text, &opts);
        Ok(serde_json::json!({ "text": text, "result": result }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 读取剪贴板图片并转 PNG（仅在用户点击按钮时调用）。
fn clipboard_image_png() -> Result<Option<Vec<u8>>, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("无法访问剪贴板：{e}"))?;
    let image = match clipboard.get_image() {
        Ok(image) => image,
        Err(_) => return Ok(None),
    };

    let mut png_bytes = Vec::new();
    {
        let mut encoder =
            png::Encoder::new(&mut png_bytes, image.width as u32, image.height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("PNG 编码失败：{e}"))?;
        writer
            .write_image_data(&image.bytes)
            .map_err(|e| format!("PNG 编码失败：{e}"))?;
    }
    Ok(Some(png_bytes))
}

#[tauri::command]
async fn ocr_clipboard(lang: String, to: String) -> Result<Option<serde_json::Value>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(png_bytes) = clipboard_image_png()? else {
            return Ok(None);
        };
        let cfg = Config::load();
        let text = yd_core::ocr::recognize(&png_bytes, &lang, &cfg).map_err(|e| e.to_string())?;
        let opts = QueryOptions {
            from: Some(lang),
            to: Some(to),
            mode: None,
            no_cache: false,
            fallback_bing: true,
        };
        let result = query(&text, &opts);
        Ok(Some(serde_json::json!({ "text": text, "result": result })))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn ocr_path(path: String, lang: String, to: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = std::fs::read(&path).map_err(|e| format!("读取图片失败：{e}"))?;
        let cfg = Config::load();
        let text = yd_core::ocr::recognize(&bytes, &lang, &cfg).map_err(|e| e.to_string())?;
        let opts = QueryOptions {
            from: Some(lang),
            to: Some(to),
            mode: None,
            no_cache: false,
            fallback_bing: true,
        };
        let result = query(&text, &opts);
        Ok(serde_json::json!({ "text": text, "result": result }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
struct SettingsView {
    configured: bool,
    app_key: String,
    app_secret: String,
    config_path: String,
}

#[tauri::command]
fn get_settings() -> SettingsView {
    let cfg = Config::load();
    SettingsView {
        configured: cfg.has_credentials(),
        app_key: cfg.app_key,
        app_secret: cfg.app_secret,
        config_path: Config::config_path().display().to_string(),
    }
}

#[tauri::command]
fn save_settings(app_key: String, app_secret: String) -> Result<String, String> {
    let path = Config::save(&app_key, &app_secret).map_err(|e| e.to_string())?;
    cache::clear();
    Ok(path.display().to_string())
}

#[tauri::command]
fn check_credentials() -> Result<String, String> {
    let cfg = Config::load();
    if !cfg.has_credentials() {
        return Err("还没有填写应用ID / 应用密钥".to_string());
    }
    api::check(&cfg).map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        // 第二次启动时把已有窗口唤到前台，而不是再开一个。
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .on_window_event(|window, event| {
            // 拖放事件在 Rust 侧接收，再用 eval 调前端函数，
            // 不依赖 window.__TAURI__.event 是否可用。
            let tauri::WindowEvent::DragDrop(drag) = event else {
                return;
            };
            let Some(webview) = window.app_handle().get_webview_window(window.label()) else {
                return;
            };
            match drag {
                tauri::DragDropEvent::Enter { .. } => {
                    let _ = webview.eval("window.__ydDragEnter && window.__ydDragEnter();");
                }
                tauri::DragDropEvent::Leave => {
                    let _ = webview.eval("window.__ydDragLeave && window.__ydDragLeave();");
                }
                tauri::DragDropEvent::Drop { paths, .. } => {
                    let list: Vec<String> = paths
                        .iter()
                        .map(|p| p.to_string_lossy().to_string())
                        .collect();
                    if let Ok(json) = serde_json::to_string(&list) {
                        let _ = webview.eval(format!(
                            "window.__ydDragDrop && window.__ydDragDrop({json});"
                        ));
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            translate,
            ocr_translate,
            ocr_clipboard,
            ocr_path,
            quit_app,
            get_settings,
            save_settings,
            check_credentials
        ])
        .run(tauri::generate_context!())
        .expect("yd-gui 启动失败");
}

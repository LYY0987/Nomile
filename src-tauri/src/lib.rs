mod commands;
mod domain;
mod error;
mod infra;
mod model;
mod state;

use tauri::Manager;

use crate::infra::config::ConfigStore;
use crate::infra::history::HistoryStore;
use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // 记住窗口尺寸与位置：关闭时写盘，下次启动自动恢复；
        // 首次运行没有存档，用 tauri.conf 里的尺寸并居中。
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED,
                )
                .build(),
        )
        .setup(|app| {
            // 配置与操作日志都放在 %APPDATA%\com.nomile.app 下，
            // 卸载重装不会丢；拿不到时退到临时目录，保证应用仍能启动。
            let base = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("Nomile"));

            let config = ConfigStore::new(base.clone());
            let history = HistoryStore::new(base.join("operations"));
            app.manage(AppState::new(config, history));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan,
            commands::clear_session,
            commands::preview,
            commands::execute,
            commands::undo,
            commands::list_operations,
            commands::get_settings,
            commands::save_settings,
            commands::get_presets,
            commands::save_presets,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

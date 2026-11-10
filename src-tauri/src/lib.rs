mod ai;
mod commands;
mod domain;
mod error;
mod infra;
mod model;
mod state;

use tauri::Manager;

use crate::ai::AiService;
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
            let ai = AiService::new(base.clone());
            app.manage(AppState::new(config, history, ai));
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
            ai::commands::ai_list_providers,
            ai::commands::ai_save_provider,
            ai::commands::ai_delete_provider,
            ai::commands::ai_set_active_provider,
            ai::commands::ai_set_key,
            ai::commands::ai_delete_key,
            ai::commands::ai_list_models,
            ai::commands::ai_test_provider,
            ai::commands::ai_generate,
            ai::commands::ai_generate_map,
            ai::commands::ai_cancel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

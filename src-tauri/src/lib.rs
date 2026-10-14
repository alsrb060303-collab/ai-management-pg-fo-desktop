#[allow(dead_code)] // Filled in by Phase 14.
mod activity;
mod bridge;
mod claude_mod;
mod composer;
mod dock;
mod git;
mod github;
mod i18n;
mod library;
mod mcp;
mod memo;
mod project_docs;
mod shell;
mod sync;
mod tools;
mod tray;
mod work;

use tauri::{Manager, WindowEvent};

use dock::{Dock, SIDEBAR_LABEL};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be registered first: a second launch just shows the running app.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if app.try_state::<shell::Shell>().is_some() {
                shell::reveal(app);
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            dock::dock_status,
            dock::dock_cycle_magnet,
            dock::dock_set_pinned,
            dock::sidebar_hide,
            tools::config_scan,
            tools::tool_detect,
            tools::claude::claude_commands,
            mcp::mcp_list,
            mcp::mcp_save,
            mcp::mcp_remove,
            mcp::mcp_set_enabled,
            mcp::mcp_parse,
            mcp::mcp_recommended,
            project_docs::project_docs_list,
            project_docs::project_doc_read,
            project_docs::project_doc_write,
            project_docs::project_doc_delete,
            library::library_overview,
            library::library_set_dir,
            library::library_apply,
            library::library_remove,
            library::library_update,
            library::library_import,
            library::library_forget,
            library::config_set_enabled,
            composer::composer_catalog,
            composer::composer_refresh_external,
            composer::composer_docs,
            composer::composer_doc_load,
            composer::composer_doc_save,
            composer::composer_doc_delete,
            composer::composer_compile,
            composer::composer_plan,
            composer::composer_write,
            memo::memo_context,
            memo::memo_get,
            memo::memo_set_note,
            memo::memo_add_todo,
            memo::memo_update_todo,
            memo::memo_delete_todo,
            memo::memo_move_todo,
            memo::memo_clear_done,
            work::work_overview,
            work::work_diff,
            work::work_switch,
            work::work_create_branch,
            work::work_check_branch,
            work::work_delete_branch,
            work::work_add_worktree,
            work::work_remove_worktree,
            work::work_prune_worktrees,
            work::work_commit,
            work::work_push,
            work::work_pull,
            work::work_git_version,
            work::work_stage,
            work::work_commit_diff,
            work::work_draft_message,
            work::work_draft_tool,
            work::work_conflict,
            work::work_resolve,
            work::work_continue,
            work::work_abort,
            i18n::set_locale,
            bridge::bridge_github,
            claude_mod::claude_mod_status,
            claude_mod::claude_mod_install,
            claude_mod::claude_mod_install_recommended,
            claude_mod::claude_mod_uninstall,
            sync::sync_status,
            sync::sync_now,
            sync::sync_set_enabled,
            sync::account::account_status,
            sync::account::account_sign_up,
            sync::account::account_sign_in,
            sync::account::account_reset_password,
            sync::account::account_send_verification,
            sync::account::account_add_password,
            sync::account::account_github,
            sync::account::account_google,
            sync::account::account_google_cancel,
            sync::account::account_unlink,
            sync::account::account_sign_out,
            github::github_status,
            github::github_login_start,
            github::github_login_cancel,
            github::github_logout,
            github::github_repo,
            github::github_create_pr,
            github::github_repos,
            github::github_local_clones,
            github::github_remote,
            github::github_remote_create_branch,
            github::github_remote_delete_branch,
            github::github_remote_create_pr,
            github::github_clone,
            github::github_remote_tree,
            github::github_remote_file,
            github::github_remote_commit,
            composer::remote::composer_plan_remote,
            composer::remote::composer_write_remote,
            shell::view_mode,
            shell::view_switch,
        ])
        .setup(|app| {
            let handle = app.handle();
            Dock::manage(handle);
            shell::Shell::manage(handle);
            library::Library::manage(handle)?;
            composer::Composer::manage(handle)?;
            memo::Memo::manage(handle)?;
            github::GitHub::manage(handle);
            sync::Sync::manage(handle);
            bridge::manage(handle);
            tray::create(handle)?;
            Dock::start(handle)?;
            tray::register_shortcut(handle);
            shell::restore(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the sidebar hides it to the tray; quit from the tray menu. The
            // main window really closes and is created again when reopened.
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.label() == SIDEBAR_LABEL
            {
                api.prevent_close();
                let app = window.app_handle();
                app.state::<Dock>().set_visible(app, false);
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

mod bridge;
mod claude_config;
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
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            dock::dock_status,
            dock::dock_set_enabled,
            dock::dock_set_pinned,
            dock::sidebar_hide,
            claude_config::claude_config_scan,
            claude_config::claude_commands,
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
            i18n::set_locale,
            bridge::bridge_github,
            claude_mod::claude_mod_status,
            claude_mod::claude_mod_install,
            claude_mod::claude_mod_install_recommended,
            claude_mod::claude_mod_uninstall,
            sync::sync_status,
            sync::sync_now,
            sync::sync_set_enabled,
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

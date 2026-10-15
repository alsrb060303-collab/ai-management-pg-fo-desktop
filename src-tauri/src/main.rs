// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `<app> mcp`: Claude Code starts the app as its MCP server (stdio).
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("mcp") {
        std::process::exit(ai_management_lib::run_mcp_server(&args[1..]));
    }
    ai_management_lib::run()
}

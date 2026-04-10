// Windows 发布版不额外弹出控制台窗口，请勿删除。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    oof_plus_desktop_lib::run()
}

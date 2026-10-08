//! Núcleo do Radar de Preços: banco local, Mercado Livre e comandos da interface.

mod db;
mod ml;
mod ml_auth;
mod prices;
mod secrets;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o aplicativo");
}

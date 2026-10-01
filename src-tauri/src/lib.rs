use serde::Serialize;

#[derive(Serialize)]
struct SiteCheck {
  ok: bool,
  status: Option<u16>,
  detail: String,
}

#[tauri::command]
async fn check_site(url: String) -> SiteCheck {
  let client = match reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(10))
    .user_agent("FigureSearch/0.1")
    .redirect(reqwest::redirect::Policy::limited(5))
    .build()
  {
    Ok(client) => client,
    Err(error) => return SiteCheck { ok: false, status: None, detail: error.to_string() },
  };

  match client.get(url).send().await {
    Ok(response) => {
      let status = response.status();
      SiteCheck {
        ok: status.is_success() || status.is_redirection() || status.as_u16() == 429,
        status: Some(status.as_u16()),
        detail: status.canonical_reason().unwrap_or("응답 수신").to_string(),
      }
    }
    Err(error) => SiteCheck { ok: false, status: None, detail: error.to_string() },
  }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .plugin(tauri_plugin_shell::init())
    .invoke_handler(tauri::generate_handler![check_site])
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while building tauri application");
}

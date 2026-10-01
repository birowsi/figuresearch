use encoding_rs::EUC_KR;
use serde::Serialize;

#[derive(Serialize)]
struct SiteCheck {
    ok: bool,
    status: Option<u16>,
    detail: String,
}

#[derive(Serialize)]
struct ProviderFetch {
    status: u16,
    body: String,
    detail: String,
    final_url: String,
    content_type: Option<String>,
    duration_ms: u128,
}

#[derive(Serialize)]
struct EncodedQuery {
    encoded: String,
    used_fallback: bool,
}

#[tauri::command]
fn encode_euc_kr(query: String) -> EncodedQuery {
    let (encoded, _, had_errors) = EUC_KR.encode(&query);
    if had_errors {
        return EncodedQuery {
            encoded: percent_encode(query.as_bytes()),
            used_fallback: true,
        };
    }
    EncodedQuery {
        encoded: percent_encode(&encoded),
        used_fallback: false,
    }
}

fn percent_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    bytes.iter().fold(String::new(), |mut result, byte| {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(byte) {
            result.push(*byte as char);
        } else {
            result.push('%');
            result.push(HEX[(byte >> 4) as usize] as char);
            result.push(HEX[(byte & 0x0f) as usize] as char);
        }
        result
    })
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
        Err(error) => {
            return SiteCheck {
                ok: false,
                status: None,
                detail: error.to_string(),
            };
        }
    };

    match client.get(&url).send().await {
        Ok(response) => {
            let status = response.status();
            SiteCheck {
                ok: status.is_success() || status.is_redirection() || status.as_u16() == 429,
                status: Some(status.as_u16()),
                detail: status.canonical_reason().unwrap_or("응답 수신").to_string(),
            }
        }
        Err(error) => SiteCheck {
            ok: false,
            status: None,
            detail: error.to_string(),
        },
    }
}

#[tauri::command]
async fn fetch_provider(url: String) -> ProviderFetch {
    let started = std::time::Instant::now();
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("FigureSearch/0.1")
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return ProviderFetch {
                status: 0,
                body: String::new(),
                detail: error.to_string(),
                final_url: url,
                content_type: None,
                duration_ms: started.elapsed().as_millis(),
            };
        }
    };
    match client.get(&url).send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            let final_url = response.url().to_string();
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            match response.text().await {
                Ok(body) => ProviderFetch {
                    status,
                    body,
                    detail: if status == 429 {
                        "요청 제한".into()
                    } else {
                        "응답 수신".into()
                    },
                    final_url,
                    content_type,
                    duration_ms: started.elapsed().as_millis(),
                },
                Err(error) => ProviderFetch {
                    status,
                    body: String::new(),
                    detail: error.to_string(),
                    final_url,
                    content_type,
                    duration_ms: started.elapsed().as_millis(),
                },
            }
        }
        Err(error) => ProviderFetch {
            status: 0,
            body: String::new(),
            detail: error.to_string(),
            final_url: url,
            content_type: None,
            duration_ms: started.elapsed().as_millis(),
        },
    }
}

#[tauri::command]
fn write_debug_artifact(
    app: tauri::AppHandle,
    session_id: String,
    provider: String,
    response: String,
    report: String,
) -> Result<(), String> {
    use tauri::Manager;
    let safe_session = session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>();
    let safe_provider = provider
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>();
    let directory = app
        .path()
        .app_local_data_dir()
        .map_err(|error| error.to_string())?
        .join("logs")
        .join(format!("search-{safe_session}"));
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    std::fs::write(
        directory.join(format!("{safe_provider}.response.html")),
        response,
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        directory.join(format!("{safe_provider}.parse.json")),
        report,
    )
    .map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            check_site,
            encode_euc_kr,
            fetch_provider,
            write_debug_artifact
        ])
        .setup(|app| {
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Info)
                    .build(),
            )?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}

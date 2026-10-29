// tray_monitor.rs
//
// Background service that monitors Windows audio/video playback and shows
// the floating prompt modal in the bottom-right corner when playback starts/stops.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use log::info;

static IS_MONITORING: AtomicBool = AtomicBool::new(false);
static LAST_PROMPT_STATE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
static LAST_PROMPT_TIME: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

pub fn start_tray_monitor<R: Runtime>(app: AppHandle<R>) {
    if IS_MONITORING.swap(true, Ordering::SeqCst) {
        return;
    }

    tauri::async_runtime::spawn(async move {
        info!("🎧 Iniciando monitor de áudio/vídeo em segundo plano...");

        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;

            let pool = match app.try_state::<crate::state::AppState>() {
                Some(state) => state.db_manager.pool().clone(),
                None => continue,
            };

            let enabled = crate::database::repositories::setting::SettingsRepository::get_tray_monitoring_enabled(&pool)
                .await
                .unwrap_or(true);

            if !enabled {
                continue;
            }

            let countdown = crate::database::repositories::setting::SettingsRepository::get_auto_record_countdown(&pool)
                .await
                .unwrap_or(5);

            // Execute check_media.ps1
            let output = tokio::process::Command::new("powershell")
                .args(&[
                    "-ExecutionPolicy", "Bypass",
                    "-NoProfile",
                    "-Command",
                    r#"
Add-Type -AssemblyName System.Runtime.WindowsRuntime
$asTaskGeneric = ([System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object { $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' })[0]
Function Await($WinRtTask, $ResultType) {
    $asTask = $asTaskGeneric.MakeGenericMethod($ResultType)
    $netTask = $asTask.Invoke($null, @($WinRtTask))
    $netTask.Wait(1000) | Out-Null
    $netTask.Result
}
try {
    [Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager, Windows.Media, ContentType = WindowsRuntime] | Out-Null
    $manager = Await ([Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager]::RequestAsync()) ([Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager])
    $sessions = $manager.GetSessions()
    $playingApp = $null
    foreach ($s in $sessions) {
        $info = $s.GetPlaybackInfo()
        if ($info.PlaybackStatus -eq 4) {
            $rawId = $s.SourceAppUserModelId
            if ($rawId -match "Spotify") { $playingApp = "Spotify" }
            elseif ($rawId -match "Chrome") { $playingApp = "Google Chrome" }
            elseif ($rawId -match "Edge") { $playingApp = "Microsoft Edge" }
            elseif ($rawId -match "Zoom") { $playingApp = "Zoom" }
            elseif ($rawId -match "Teams") { $playingApp = "Microsoft Teams" }
            elseif ($rawId -match "WhatsApp") { $playingApp = "WhatsApp" }
            else {
                $parts = $rawId -split "\\|\.|\!"
                $playingApp = $parts[-1]
            }
            break
        }
    }
    if ($playingApp) { Write-Output "PLAYING:$playingApp" } else { Write-Output "STOPPED" }
} catch { Write-Output "STOPPED" }
"#,
                ])
                .output()
                .await;

            let media_status = match output {
                Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
                Err(_) => "STOPPED".to_string(),
            };

            let is_recording = crate::is_recording().await;

            if media_status.starts_with("PLAYING:") {
                let app_name = media_status.strip_prefix("PLAYING:").unwrap_or("Mídia");
                if !is_recording {
                    let should_prompt = {
                        let mut last_state = LAST_PROMPT_STATE.lock().unwrap();
                        let mut last_time = LAST_PROMPT_TIME.lock().unwrap();
                        let now = std::time::Instant::now();

                        let can_prompt = match (*last_state).as_deref() {
                            Some("start") => {
                                if let Some(t) = *last_time {
                                    now.duration_since(t).as_secs() > 60
                                } else {
                                    true
                                }
                            }
                            _ => true,
                        };

                        if can_prompt {
                            *last_state = Some("start".to_string());
                            *last_time = Some(now);
                            true
                        } else {
                            false
                        }
                    };

                    if should_prompt {
                        info!("🎬 Mídia detectada: {}. Exibindo modal no canto inferior...", app_name);
                        show_prompt_window(&app, "start", app_name, countdown);
                    }
                }
            } else {
                if is_recording {
                    let should_prompt = {
                        let mut last_state = LAST_PROMPT_STATE.lock().unwrap();
                        let mut last_time = LAST_PROMPT_TIME.lock().unwrap();
                        let now = std::time::Instant::now();

                        let can_prompt = match (*last_state).as_deref() {
                            Some("stop") => false,
                            _ => true,
                        };

                        if can_prompt {
                            *last_state = Some("stop".to_string());
                            *last_time = Some(now);
                            true
                        } else {
                            false
                        }
                    };

                    if should_prompt {
                        info!("🛑 Mídia encerrada enquanto gravava. Exibindo modal para parar...");
                        show_prompt_window(&app, "stop", "", 0);
                    }
                } else {
                    let mut last_state = LAST_PROMPT_STATE.lock().unwrap();
                    if last_state.as_deref() == Some("stop") {
                        *last_state = None;
                    }
                }
            }
        }
    });
}

pub fn show_prompt_window<R: Runtime>(app: &AppHandle<R>, mode: &str, app_name: &str, countdown: i64) {
    if let Some(win) = app.get_webview_window("prompt") {
        let _ = win.emit("show-prompt", serde_json::json!({
            "mode": mode,
            "appName": app_name,
            "autoRecordCountdown": countdown,
        }));

        if let Some(monitor) = win.current_monitor().ok().flatten().or_else(|| win.primary_monitor().ok().flatten()) {
            let screen_size = monitor.size();
            let scale_factor = monitor.scale_factor();
            let win_w = (380.0 * scale_factor) as i32;
            let win_h = (180.0 * scale_factor) as i32;
            let margin_x = (24.0 * scale_factor) as i32;
            let margin_y = (64.0 * scale_factor) as i32;

            let x = (screen_size.width as i32) - win_w - margin_x;
            let y = (screen_size.height as i32) - win_h - margin_y;

            let _ = win.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
        }

        let _ = win.show();
        let _ = win.set_focus();
    }
}

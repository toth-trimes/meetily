// tray_monitor.rs
//
// Background service that monitors Windows audio/video playback and shows
// the floating prompt modal in the bottom-right corner when playback starts/stops.

use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use log::info;
use tokio::io::{AsyncBufReadExt, BufReader};

static IS_MONITORING: AtomicBool = AtomicBool::new(false);
static LAST_PROMPT_STATE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
static LAST_PROMPT_TIME: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

#[cfg(not(target_os = "windows"))]
pub fn start_tray_monitor<R: Runtime>(_app: AppHandle<R>) {
    // Media transport control monitoring is only supported on Windows
}

#[cfg(target_os = "windows")]
pub fn start_tray_monitor<R: Runtime>(app: AppHandle<R>) {
    if IS_MONITORING.swap(true, Ordering::SeqCst) {
        return;
    }

    tauri::async_runtime::spawn(async move {
        info!("🎧 Iniciando monitor de áudio/vídeo em segundo plano...");

        loop {
            let pool = match app.try_state::<crate::state::AppState>() {
                Some(state) => state.db_manager.pool().clone(),
                None => {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };

            let enabled = crate::database::repositories::setting::SettingsRepository::get_tray_monitoring_enabled(&pool)
                .await
                .unwrap_or(false);

            if !enabled {
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }

            let mut cmd = tokio::process::Command::new("powershell");
            cmd.args(&[
                "-ExecutionPolicy", "Bypass",
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle", "Hidden",
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
} catch {}

while ($true) {
    try {
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
        if ($playingApp) { [Console]::WriteLine("PLAYING:$playingApp") } else { [Console]::WriteLine("STOPPED") }
    } catch {
        [Console]::WriteLine("STOPPED")
    }
    Start-Sleep -Seconds 4
}
"#,
            ]);
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::null());
            cmd.stdin(Stdio::null());

            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);

            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    log::error!("Falha ao iniciar processo de monitoramento: {}", e);
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    continue;
                }
            };

            if let Some(stdout) = child.stdout.take() {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let enabled = crate::database::repositories::setting::SettingsRepository::get_tray_monitoring_enabled(&pool)
                        .await
                        .unwrap_or(false);
                    if !enabled {
                        let _ = child.kill().await;
                        break;
                    }

                    let media_status = line.trim().to_string();
                    let countdown = crate::database::repositories::setting::SettingsRepository::get_auto_record_countdown(&pool)
                        .await
                        .unwrap_or(5);

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
            }

            let _ = child.kill().await;
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

pub fn show_prompt_window<R: Runtime>(app: &AppHandle<R>, mode: &str, app_name: &str, countdown: i64) {
    let win = if let Some(w) = app.get_webview_window("prompt") {
        w
    } else {
        match tauri::WebviewWindowBuilder::new(
            app,
            "prompt",
            tauri::WebviewUrl::App("/prompt".into()),
        )
        .title("Meetily Prompt")
        .inner_size(380.0, 220.0)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .transparent(true)
        .visible(false)
        .build()
        {
            Ok(w) => w,
            Err(e) => {
                log::error!("Failed to create prompt window: {}", e);
                return;
            }
        }
    };

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

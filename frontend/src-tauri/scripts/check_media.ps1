# Check Windows Media Sessions for active playback
Add-Type -AssemblyName System.Runtime.WindowsRuntime
$asTaskGeneric = ([System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object { 
    $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' 
})[0]

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
        if ($info.PlaybackStatus -eq 4) { # 4 = Playing
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

    if ($playingApp) {
        Write-Output "PLAYING:$playingApp"
    } else {
        Write-Output "STOPPED"
    }
} catch {
    Write-Output "STOPPED"
}

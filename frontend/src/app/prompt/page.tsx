'use client';

import React, { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { Mic, Square, X, Check, Volume2, Sparkles, Clock } from 'lucide-react';

type PromptMode = 'start' | 'stop';

interface PromptEventData {
  mode: PromptMode;
  appName?: string;
  autoRecordCountdown?: number;
}

export default function PromptPage() {
  const [mode, setMode] = useState<PromptMode>('start');
  const [appName, setAppName] = useState<string>('');
  const [countdown, setCountdown] = useState<number>(5);
  const [initialCountdown, setInitialCountdown] = useState<number>(5);
  const [isPaused, setIsPaused] = useState<boolean>(false);
  const [sendToAi, setSendToAi] = useState<boolean>(true);
  const timerRef = useRef<NodeJS.Timeout | null>(null);

  const closeWindow = async () => {
    try {
      const win = getCurrentWebviewWindow();
      await win.hide();
    } catch (e) {
      console.error('Failed to hide prompt window:', e);
    }
  };

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    const setup = async () => {
      unlisten = await listen<PromptEventData>('show-prompt', (event) => {
        const data = event.payload;
        setMode(data.mode);
        setAppName(data.appName || 'Aplicativo');
        const count = data.autoRecordCountdown ?? 5;
        setCountdown(count);
        setInitialCountdown(count);
        setIsPaused(false);
      });
    };

    setup();
    return () => {
      unlisten?.();
    };
  }, []);

  // Countdown timer for auto-start
  useEffect(() => {
    if (mode !== 'start' || isPaused || countdown <= 0) return;

    timerRef.current = setTimeout(() => {
      setCountdown((prev) => {
        if (prev <= 1) {
          // Trigger auto-start!
          handleStartRecording();
          return 0;
        }
        return prev - 1;
      });
    }, 1000);

    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [mode, isPaused, countdown]);

  const handleStartRecording = async () => {
    try {
      if (timerRef.current) clearTimeout(timerRef.current);
      await invoke('api_tray_prompt_action', {
        action: 'start',
        sendToAi: true,
      });
    } catch (e) {
      console.error('Failed to start recording via prompt:', e);
    } finally {
      await closeWindow();
    }
  };

  const handleCancelStart = async () => {
    if (timerRef.current) clearTimeout(timerRef.current);
    try {
      await invoke('api_tray_prompt_action', {
        action: 'dismiss',
        sendToAi: false,
      });
    } catch (e) {
      console.error('Failed to dismiss prompt:', e);
    } finally {
      await closeWindow();
    }
  };

  const handleStopRecording = async () => {
    try {
      await invoke('api_tray_prompt_action', {
        action: 'stop',
        sendToAi: sendToAi,
      });
    } catch (e) {
      console.error('Failed to stop recording via prompt:', e);
    } finally {
      await closeWindow();
    }
  };

  const progressPercent = initialCountdown > 0
    ? ((initialCountdown - countdown) / initialCountdown) * 100
    : 0;

  return (
    <div className="w-full h-screen bg-slate-900/95 text-slate-100 p-4 rounded-xl border border-slate-700/80 shadow-2xl backdrop-blur-md flex flex-col justify-between select-none">
      {/* Top Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          {mode === 'start' ? (
            <div className="p-1.5 rounded-lg bg-blue-500/20 text-blue-400">
              <Volume2 className="w-4 h-4 animate-pulse" />
            </div>
          ) : (
            <div className="p-1.5 rounded-lg bg-emerald-500/20 text-emerald-400">
              <Clock className="w-4 h-4" />
            </div>
          )}
          <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">
            Meetily Assistente
          </span>
        </div>
        <button
          onClick={closeWindow}
          className="text-slate-400 hover:text-white p-1 rounded hover:bg-slate-800 transition-colors"
        >
          <X className="w-3.5 h-3.5" />
        </button>
      </div>

      {/* Main Content Area */}
      {mode === 'start' ? (
        <div className="my-1">
          <h2 className="text-sm font-medium text-slate-100">
            Áudio detectado em <span className="text-blue-400 font-semibold">{appName || 'Mídia'}</span>
          </h2>
          <p className="text-xs text-slate-400 mt-0.5">
            Deseja iniciar a gravação da reunião?
          </p>

          {/* Countdown bar */}
          <div className="mt-3">
            <div className="flex justify-between text-[11px] text-slate-400 mb-1">
              <span>Gravação automática em</span>
              <span className="font-mono font-medium text-blue-400">{countdown}s</span>
            </div>
            <div className="w-full bg-slate-800 rounded-full h-1.5 overflow-hidden">
              <div
                className="bg-blue-500 h-1.5 rounded-full transition-all duration-1000 ease-linear"
                style={{ width: `${progressPercent}%` }}
              />
            </div>
          </div>
        </div>
      ) : (
        <div className="my-1">
          <h2 className="text-sm font-medium text-slate-100">
            O áudio parece ter terminado
          </h2>
          <p className="text-xs text-slate-400 mt-0.5">
            Deseja finalizar esta gravação agora?
          </p>

          <label className="flex items-center gap-2 mt-3 cursor-pointer group">
            <input
              type="checkbox"
              checked={sendToAi}
              onChange={(e) => setSendToAi(e.target.checked)}
              className="rounded bg-slate-800 border-slate-700 text-blue-500 focus:ring-0 cursor-pointer"
            />
            <span className="text-xs text-slate-300 group-hover:text-white flex items-center gap-1.5">
              <Sparkles className="w-3.5 h-3.5 text-blue-400" />
              Enviar para transcrição e resumo com IA
            </span>
          </label>
        </div>
      )}

      {/* Actions */}
      <div className="flex items-center justify-end gap-2 pt-2 border-t border-slate-800/80">
        {mode === 'start' ? (
          <>
            <button
              onClick={handleCancelStart}
              className="px-3 py-1.5 rounded-lg text-xs font-medium text-slate-300 hover:bg-slate-800 transition-colors"
            >
              Agora não
            </button>
            <button
              onClick={handleStartRecording}
              className="px-3 py-1.5 rounded-lg text-xs font-medium bg-blue-600 hover:bg-blue-500 text-white flex items-center gap-1.5 shadow-md shadow-blue-900/30 transition-all"
            >
              <Mic className="w-3.5 h-3.5" />
              Gravar Agora
            </button>
          </>
        ) : (
          <>
            <button
              onClick={closeWindow}
              className="px-3 py-1.5 rounded-lg text-xs font-medium text-slate-300 hover:bg-slate-800 transition-colors"
            >
              Continuar
            </button>
            <button
              onClick={handleStopRecording}
              className="px-3 py-1.5 rounded-lg text-xs font-medium bg-emerald-600 hover:bg-emerald-500 text-white flex items-center gap-1.5 shadow-md shadow-emerald-900/30 transition-all"
            >
              <Square className="w-3.5 h-3.5" />
              Encerrar
            </button>
          </>
        )}
      </div>
    </div>
  );
}

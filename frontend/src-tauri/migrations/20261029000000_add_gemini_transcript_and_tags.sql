-- Migration: Add Gemini support to transcript settings, tags to meetings, and tray monitoring settings
ALTER TABLE transcript_settings ADD COLUMN geminiApiKey TEXT;
ALTER TABLE meetings ADD COLUMN tags TEXT;
ALTER TABLE settings ADD COLUMN trayMonitoringEnabled INTEGER DEFAULT 1;
ALTER TABLE settings ADD COLUMN autoRecordCountdown INTEGER DEFAULT 5;

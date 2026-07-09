// Typed bridge to the Rust recording commands + events.
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type StreamKind = 'mic' | 'system';

export interface LevelEvent {
  stream: StreamKind;
  rms: number;
}

export interface DurationEvent {
  seconds: number;
}

export interface StreamErrorEvent {
  stream: StreamKind;
  message: string;
}

export interface ChunkSummary {
  stream: StreamKind;
  index: number;
  path: string;
  startMs: number;
  durationMs: number;
  silent: boolean;
}

export interface StartedRecording {
  meetingId: string;
  recordingsDir: string;
}

export interface StoppedRecording {
  meetingId: string;
  durationSeconds: number;
  micChunks: ChunkSummary[];
  systemChunks: ChunkSummary[];
}

export interface RecordingStatus {
  recording: boolean;
  meetingId: string | null;
  durationSeconds: number;
}

export const startRecording = (): Promise<StartedRecording> => invoke('start_recording');
export const stopRecording = (): Promise<StoppedRecording> => invoke('stop_recording');
export const recordingStatus = (): Promise<RecordingStatus> => invoke('recording_status');
export const systemAudioAvailable = (): Promise<boolean> => invoke('system_audio_available');
export const micAvailable = (): Promise<boolean> => invoke('mic_available');

export const onLevel = (fn: (e: LevelEvent) => void): Promise<UnlistenFn> =>
  listen<LevelEvent>('recording://level', (e) => fn(e.payload));
export const onDuration = (fn: (e: DurationEvent) => void): Promise<UnlistenFn> =>
  listen<DurationEvent>('recording://duration', (e) => fn(e.payload));
export const onChunk = (fn: (e: ChunkSummary) => void): Promise<UnlistenFn> =>
  listen<ChunkSummary>('recording://chunk', (e) => fn(e.payload));
export const onStreamError = (fn: (e: StreamErrorEvent) => void): Promise<UnlistenFn> =>
  listen<StreamErrorEvent>('recording://error', (e) => fn(e.payload));

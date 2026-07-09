// Typed bridge to Rust transcription/model/meeting commands + events.
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface Segment {
  speaker: string;
  text: string;
  startMs: number;
  endMs: number;
}

export interface Meeting {
  id: string;
  title: string;
  createdAt: string;
  durationSeconds: number;
  status: 'recording' | 'transcribing' | 'ready';
  whisperModel: string | null;
}

export interface ModelInfo {
  name: string;
  sizeMb: number;
  downloaded: boolean;
}

export interface ChunkDoneEvent {
  meetingId: string;
  stream: string;
  idx: number;
  segments: Segment[];
}

export interface ChunkFailedEvent {
  meetingId: string;
  stream: string;
  idx: number;
  error: string;
}

export interface MeetingReadyEvent {
  meetingId: string;
}

export interface ModelProgress {
  model: string;
  downloadedBytes: number;
  totalBytes: number;
}

export const listMeetings = (): Promise<Meeting[]> => invoke('list_meetings');
export const getTranscript = (meetingId: string): Promise<Segment[]> =>
  invoke('get_transcript', { meetingId });
export const listModels = (): Promise<ModelInfo[]> => invoke('list_models');
export const downloadModel = (model: string): Promise<void> => invoke('download_model', { model });
export const getWhisperModel = (): Promise<string> => invoke('get_whisper_model');
export const setWhisperModel = (model: string): Promise<void> =>
  invoke('set_whisper_model', { model });

export const onChunkDone = (fn: (e: ChunkDoneEvent) => void): Promise<UnlistenFn> =>
  listen<ChunkDoneEvent>('transcribe://chunk-done', (e) => fn(e.payload));
export const onChunkFailed = (fn: (e: ChunkFailedEvent) => void): Promise<UnlistenFn> =>
  listen<ChunkFailedEvent>('transcribe://chunk-failed', (e) => fn(e.payload));
export const onMeetingReady = (fn: (e: MeetingReadyEvent) => void): Promise<UnlistenFn> =>
  listen<MeetingReadyEvent>('transcribe://meeting-ready', (e) => fn(e.payload));
export const onModelProgress = (fn: (e: ModelProgress) => void): Promise<UnlistenFn> =>
  listen<ModelProgress>('model://progress', (e) => fn(e.payload));

export function formatTimestamp(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const m = Math.floor(totalSeconds / 60);
  const s = totalSeconds % 60;
  return `${m}:${String(s).padStart(2, '0')}`;
}

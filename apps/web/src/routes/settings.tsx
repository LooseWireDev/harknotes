import { useEffect, useState } from 'react';
import { createFileRoute } from '@tanstack/react-router';

import { getAiSettings, setAiSettings, type AiSettings } from '../lib/summarize';
import {
  downloadModel,
  getWhisperModel,
  listModels,
  onModelProgress,
  setWhisperModel,
  type ModelInfo,
} from '../lib/transcription';

export const Route = createFileRoute('/settings')({
  component: SettingsPage,
});

const PRESETS: Array<{ label: string; baseUrl: string; model: string }> = [
  { label: 'Ollama (local)', baseUrl: 'http://localhost:11434/v1', model: 'qwen3:4b' },
  { label: 'LM Studio (local)', baseUrl: 'http://localhost:1234/v1', model: '' },
  { label: 'llama.cpp server (local)', baseUrl: 'http://localhost:8080/v1', model: '' },
  { label: 'OpenRouter', baseUrl: 'https://openrouter.ai/api/v1', model: 'meta-llama/llama-3.3-70b-instruct' },
  { label: 'Groq', baseUrl: 'https://api.groq.com/openai/v1', model: 'llama-3.3-70b-versatile' },
  { label: 'OpenAI', baseUrl: 'https://api.openai.com/v1', model: 'gpt-4o-mini' },
];

function SettingsPage(): React.ReactElement {
  const [ai, setAi] = useState<AiSettings | null>(null);
  const [saved, setSaved] = useState(false);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [whisper, setWhisper] = useState('base');
  const [downloadPct, setDownloadPct] = useState<number | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void getAiSettings().then(setAi);
    void listModels().then(setModels);
    void getWhisperModel().then(setWhisper);
    void onModelProgress((e) => {
      if (e.totalBytes > 0 && e.downloadedBytes >= e.totalBytes) {
        setDownloadPct(null);
        void listModels().then(setModels);
      } else {
        setDownloadPct(e.totalBytes > 0 ? Math.round((e.downloadedBytes / e.totalBytes) * 100) : 0);
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const save = async (): Promise<void> => {
    if (!ai) return;
    await setAiSettings(ai);
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  const selectedWhisper = models.find((m) => m.name === whisper);

  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-8 p-8">
      <h1 className="text-2xl font-semibold">Settings</h1>

      <section className="flex flex-col gap-3">
        <h2 className="font-medium">Transcription</h2>
        <label className="flex items-center gap-3 text-sm">
          <span className="w-32 text-neutral-500">Whisper model</span>
          <select
            value={whisper}
            onChange={(e) => {
              setWhisper(e.target.value);
              void setWhisperModel(e.target.value);
            }}
            className="rounded-md border border-neutral-300 bg-white px-2 py-1"
          >
            {models.map((m) => (
              <option key={m.name} value={m.name}>
                {m.name} ({m.sizeMb} MB){m.downloaded ? ' ✓' : ''}
              </option>
            ))}
          </select>
          {selectedWhisper && !selectedWhisper.downloaded && (
            <button
              type="button"
              onClick={() => void downloadModel(whisper)}
              className="rounded bg-neutral-800 px-3 py-1 text-white hover:bg-neutral-700"
            >
              {downloadPct === null ? 'Download' : `${downloadPct}%`}
            </button>
          )}
        </label>
        <p className="text-xs text-neutral-400">
          Runs fully on this machine. Smaller models are faster; larger ones are more accurate.
        </p>
      </section>

      <section className="flex flex-col gap-3">
        <h2 className="font-medium">Summarization (AI endpoint)</h2>
        <p className="text-xs text-neutral-400">
          Any OpenAI-compatible endpoint works — a local Ollama/LM Studio/llama.cpp server keeps
          everything on this machine, or bring your own key for a cloud provider. Only transcript
          text is ever sent; audio never leaves your computer.
        </p>

        <div className="flex flex-wrap gap-2">
          {PRESETS.map((p) => (
            <button
              key={p.label}
              type="button"
              onClick={() =>
                ai &&
                setAi({ ...ai, baseUrl: p.baseUrl, model: p.model || ai.model })
              }
              className="rounded-full border border-neutral-300 px-3 py-1 text-xs text-neutral-600 hover:bg-neutral-100"
            >
              {p.label}
            </button>
          ))}
        </div>

        {ai && (
          <div className="flex flex-col gap-2 text-sm">
            <label className="flex items-center gap-3">
              <span className="w-32 text-neutral-500">Base URL</span>
              <input
                value={ai.baseUrl}
                onChange={(e) => setAi({ ...ai, baseUrl: e.target.value })}
                className="flex-1 rounded-md border border-neutral-300 px-2 py-1 font-mono text-xs"
              />
            </label>
            <label className="flex items-center gap-3">
              <span className="w-32 text-neutral-500">API key</span>
              <input
                type="password"
                value={ai.apiKey}
                onChange={(e) => setAi({ ...ai, apiKey: e.target.value })}
                placeholder="not needed for local endpoints"
                className="flex-1 rounded-md border border-neutral-300 px-2 py-1 font-mono text-xs"
              />
            </label>
            <label className="flex items-center gap-3">
              <span className="w-32 text-neutral-500">Model</span>
              <input
                value={ai.model}
                onChange={(e) => setAi({ ...ai, model: e.target.value })}
                placeholder="e.g. qwen3:4b, llama3.2:3b, gemma3:4b"
                className="flex-1 rounded-md border border-neutral-300 px-2 py-1 font-mono text-xs"
              />
            </label>
            <div className="flex items-center gap-3">
              <button
                type="button"
                onClick={() => void save()}
                className="w-fit rounded-md bg-emerald-600 px-4 py-1.5 font-medium text-white hover:bg-emerald-700"
              >
                Save
              </button>
              {saved && <span className="text-emerald-600">Saved ✓</span>}
            </div>
          </div>
        )}
      </section>
    </div>
  );
}

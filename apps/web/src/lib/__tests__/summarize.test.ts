import { describe, expect, it } from 'vitest';

import { coerceSummary, extractJson, truncateTranscript } from '../summarize';

describe('extractJson', () => {
  it('parses bare JSON', () => {
    expect(extractJson('{"title":"x"}')).toEqual({ title: 'x' });
  });

  it('parses fenced JSON with prose around it', () => {
    const text = 'Sure! Here is the summary:\n```json\n{"title":"x"}\n```\nHope that helps!';
    expect(extractJson(text)).toEqual({ title: 'x' });
  });

  it('parses JSON embedded in prose without fences', () => {
    expect(extractJson('The result is {"a":1} as requested.')).toEqual({ a: 1 });
  });

  it('throws when there is no object', () => {
    expect(() => extractJson('no json here')).toThrow();
  });
});

describe('coerceSummary', () => {
  it('fills missing fields with defaults', () => {
    const s = coerceSummary({ title: 'T', summary: 'S' });
    expect(s.title).toBe('T');
    expect(s.keyTopics).toEqual([]);
    expect(s.actionItems).toEqual([]);
    expect(s.decisions).toEqual([]);
    expect(s.openQuestions).toEqual([]);
  });

  it('keeps valid structured fields', () => {
    const s = coerceSummary({
      keyTopics: [{ topic: 'A', detail: 'B' }],
      actionItems: [{ speaker: 'User', action: 'ship it' }],
    });
    expect(s.keyTopics).toHaveLength(1);
    expect(s.actionItems[0]?.action).toBe('ship it');
    expect(s.title).toBe('Meeting summary');
  });

  it('rejects structurally wrong output', () => {
    expect(() => coerceSummary({ keyTopics: 'not an array' })).toThrow();
  });
});

describe('truncateTranscript', () => {
  it('returns short transcripts unchanged', () => {
    expect(truncateTranscript('a\nb\nc', 100)).toBe('a\nb\nc');
  });

  it('cuts at line boundaries under the cap', () => {
    const lines = Array.from({ length: 100 }, (_, i) => `line ${i} ${'x'.repeat(50)}`);
    const out = truncateTranscript(lines.join('\n'), 100); // ~400 chars
    expect(out.length).toBeLessThanOrEqual(400);
    expect(out.endsWith('x')).toBe(true); // whole line kept
    expect(out.split('\n').every((l) => l.startsWith('line '))).toBe(true);
  });
});

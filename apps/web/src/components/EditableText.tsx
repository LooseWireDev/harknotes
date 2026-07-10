import { useEffect, useRef, useState } from 'react';

import { cn } from '@/lib/utils';

/**
 * Click-to-edit text. Single-line uses an input (Enter saves); multiline uses
 * an auto-growing textarea (Enter saves, Shift+Enter newline). Escape cancels.
 */
export function EditableText({
  value,
  onSave,
  multiline = false,
  className,
  editClassName,
  placeholder,
}: {
  value: string;
  onSave: (next: string) => void;
  multiline?: boolean;
  className?: string;
  editClassName?: string;
  placeholder?: string;
}): React.ReactElement {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);
  const ref = useRef<HTMLTextAreaElement & HTMLInputElement>(null);

  useEffect(() => setDraft(value), [value]);

  useEffect(() => {
    if (editing && ref.current) {
      ref.current.focus();
      if (multiline) {
        ref.current.style.height = 'auto';
        ref.current.style.height = `${ref.current.scrollHeight}px`;
      } else {
        ref.current.select();
      }
    }
  }, [editing, multiline]);

  const save = (): void => {
    setEditing(false);
    const trimmed = draft.trim();
    if (trimmed && trimmed !== value) onSave(trimmed);
    else setDraft(value);
  };

  const cancel = (): void => {
    setDraft(value);
    setEditing(false);
  };

  const keyHandler = (e: React.KeyboardEvent): void => {
    if (e.key === 'Enter' && (!multiline || !e.shiftKey)) {
      e.preventDefault();
      save();
    }
    if (e.key === 'Escape') cancel();
  };

  if (editing) {
    const shared = {
      ref,
      value: draft,
      onBlur: save,
      onKeyDown: keyHandler,
      className: cn(
        'w-full rounded-md border border-border bg-transparent px-2 py-1 outline-none focus:border-mint/50',
        className,
        editClassName,
      ),
    };
    return multiline ? (
      <textarea
        {...shared}
        rows={1}
        onChange={(e) => {
          setDraft(e.target.value);
          e.target.style.height = 'auto';
          e.target.style.height = `${e.target.scrollHeight}px`;
        }}
        style={{ resize: 'none' }}
      />
    ) : (
      <input {...shared} onChange={(e) => setDraft(e.target.value)} />
    );
  }

  return (
    <span
      role="button"
      tabIndex={0}
      title="Click to edit"
      onClick={() => setEditing(true)}
      onKeyDown={(e) => e.key === 'Enter' && setEditing(true)}
      className={cn(
        '-mx-1 cursor-text rounded px-1 transition-colors hover:bg-bg-elevated',
        !value && 'italic text-text-tertiary',
        className,
      )}
    >
      {value || placeholder || 'Click to add'}
    </span>
  );
}

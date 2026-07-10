import { useEffect, useRef } from 'react';

// Ported from the legacy RecordingOverlay: a scrolling equalizer where new
// levels enter on the right and older bars fade out to the left.
const WAVEFORM_BARS = 40;
const WAVEFORM_BAR_WIDTH = 3;
const WAVEFORM_GAP = 2;
const WAVEFORM_HEIGHT = 120;

export function WaveformVisualizer({
  levelRef,
  color,
  label,
}: {
  levelRef: React.RefObject<number>;
  color: string;
  label: string;
}): React.ReactElement {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const barsRef = useRef<number[]>(new Array(WAVEFORM_BARS).fill(0));
  const animationRef = useRef<number>(0);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const totalWidth = WAVEFORM_BARS * WAVEFORM_BAR_WIDTH + (WAVEFORM_BARS - 1) * WAVEFORM_GAP;
    canvas.width = totalWidth;
    canvas.height = WAVEFORM_HEIGHT;

    const draw = (): void => {
      const bars = barsRef.current;
      const level = levelRef.current ?? 0;

      bars.shift();
      // Amplify and clamp — RMS is typically 0-0.1 for speech.
      bars.push(Math.min(1, level * 8));

      ctx.clearRect(0, 0, canvas.width, canvas.height);
      for (let i = 0; i < WAVEFORM_BARS; i++) {
        const barHeight = Math.max(2, (bars[i] ?? 0) * WAVEFORM_HEIGHT * 0.8);
        const x = i * (WAVEFORM_BAR_WIDTH + WAVEFORM_GAP);
        const y = (WAVEFORM_HEIGHT - barHeight) / 2;

        // Fade bars from left to right (older = dimmer).
        const alpha = 0.3 + (i / WAVEFORM_BARS) * 0.7;
        ctx.fillStyle = `${color}${Math.round(alpha * 255)
          .toString(16)
          .padStart(2, '0')}`;
        ctx.beginPath();
        ctx.roundRect(x, y, WAVEFORM_BAR_WIDTH, barHeight, 1.5);
        ctx.fill();
      }

      animationRef.current = requestAnimationFrame(draw);
    };

    draw();
    return () => cancelAnimationFrame(animationRef.current);
  }, [levelRef, color]);

  const totalWidth = WAVEFORM_BARS * WAVEFORM_BAR_WIDTH + (WAVEFORM_BARS - 1) * WAVEFORM_GAP;

  return (
    <div className="flex flex-col items-center gap-1">
      <canvas ref={canvasRef} width={totalWidth} height={WAVEFORM_HEIGHT} />
      <span className="text-xs text-text-tertiary">{label}</span>
    </div>
  );
}

'use client';

const PALETTE = [
  'bg-blue-100 text-blue-800',
  'bg-emerald-100 text-emerald-800',
  'bg-amber-100 text-amber-800',
  'bg-purple-100 text-purple-800',
  'bg-rose-100 text-rose-800',
  'bg-cyan-100 text-cyan-800',
  'bg-orange-100 text-orange-800',
  'bg-slate-200 text-slate-800',
];

function colorFor(label: string): string {
  const index = Number(label.replace('SPEAKER_', '')) || 0;
  return PALETTE[index % PALETTE.length];
}

export function speakerText(label: string, displayName?: string | null): string {
  return displayName?.trim() || `说话人 ${Number(label.replace('SPEAKER_', '')) + 1 || 1}`;
}

export function SpeakerBadge({
  label,
  displayName,
  onClick,
}: {
  label: string;
  displayName?: string | null;
  onClick?: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={onClick ? '重命名说话人' : undefined}
      className={`shrink-0 rounded px-1.5 py-0.5 text-xs font-medium ${colorFor(label)} ${onClick ? 'cursor-pointer hover:opacity-80' : 'cursor-default'}`}
    >
      {speakerText(label, displayName)}
    </button>
  );
}

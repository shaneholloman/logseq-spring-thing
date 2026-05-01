import React, { useState, useCallback, useEffect, useRef } from 'react';
import {
  Settings,
  FileText,
  Database,
  Code,
  GraduationCap,
  Check,
  Wand2,
  type LucideIcon,
} from 'lucide-react';
import { cn } from '@/utils/classNameUtils';
import { useWebSocketStore } from '@/store/websocketStore';
import { createLogger } from '@/utils/loggerConfig';

const logger = createLogger('ForcePresetSelector');

// ── Types ──────────────────────────────────────────────────────────────

interface ForcePreset {
  id: string;
  label: string;
  icon: LucideIcon;
  description: string;
  nodeCountHint: string;
}

export interface ForcePresetSelectorProps {
  className?: string;
  compact?: boolean;
  onPresetChange?: (presetId: string) => void;
}

// ── Preset definitions (mirrors Rust ForcePreset enum) ─────────────────

const FORCE_PRESETS: ForcePreset[] = [
  {
    id: 'default',
    label: 'Default',
    icon: Settings,
    description: 'General-purpose balanced forces',
    nodeCountHint: 'Any size',
  },
  {
    id: 'logseq_small',
    label: 'Logseq Small',
    icon: FileText,
    description: 'Tuned for small personal knowledge graphs',
    nodeCountHint: '≤1k nodes',
  },
  {
    id: 'logseq_large',
    label: 'Logseq Large',
    icon: Database,
    description: 'Optimised for large knowledge bases',
    nodeCountHint: '1k–100k nodes',
  },
  {
    id: 'code_repo',
    label: 'Code Repo',
    icon: Code,
    description: 'Hierarchical layout for source code graphs',
    nodeCountHint: 'Any size',
  },
  {
    id: 'research_wiki',
    label: 'Research Wiki',
    icon: GraduationCap,
    description: 'Dense clusters for citation / wiki networks',
    nodeCountHint: '1k–100k nodes',
  },
];

const EASE_IN_MS = 1000; // 60 frames at 60 fps
const STORAGE_KEY = 'force-preset';
const AUTO_STORAGE_KEY = 'force-preset-auto';

// ── Component ──────────────────────────────────────────────────────────

export const ForcePresetSelector: React.FC<ForcePresetSelectorProps> = ({
  className,
  compact = false,
  onPresetChange,
}) => {
  const sendMessage = useWebSocketStore((s) => s.sendMessage);

  const [activeId, setActiveId] = useState<string>(
    () => localStorage.getItem(STORAGE_KEY) ?? 'default',
  );
  const [transitioning, setTransitioning] = useState(false);
  const [autoSelect, setAutoSelect] = useState<boolean>(
    () => localStorage.getItem(AUTO_STORAGE_KEY) === 'true',
  );
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Persist auto toggle
  useEffect(() => {
    localStorage.setItem(AUTO_STORAGE_KEY, String(autoSelect));
  }, [autoSelect]);

  const selectPreset = useCallback(
    (presetId: string) => {
      if (transitioning || presetId === activeId) return;

      setActiveId(presetId);
      setTransitioning(true);
      localStorage.setItem(STORAGE_KEY, presetId);

      // Notify backend via WebSocket
      sendMessage('set_force_preset', { preset: presetId });
      logger.info(`Force preset changed to ${presetId}`);

      onPresetChange?.(presetId);

      // Clear any previous timer, then schedule end of ease-in indicator
      if (timerRef.current) clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => setTransitioning(false), EASE_IN_MS);
    },
    [activeId, transitioning, sendMessage, onPresetChange],
  );

  // Cleanup timer on unmount
  useEffect(() => () => { if (timerRef.current) clearTimeout(timerRef.current); }, []);

  // ── Auto toggle row ────────────────────────────────────────────────

  const autoToggle = (
    <button
      type="button"
      onClick={() => {
        const next = !autoSelect;
        setAutoSelect(next);
        sendMessage('set_force_preset_auto', { enabled: next });
        logger.info(`Auto force-preset selection ${next ? 'enabled' : 'disabled'}`);
      }}
      className={cn(
        'flex items-center gap-2 px-3 py-2 rounded-lg border text-sm transition-all',
        autoSelect
          ? 'border-amber-500/50 bg-amber-500/10 text-amber-300'
          : 'border-white/10 bg-white/5 text-gray-400 hover:bg-white/10',
      )}
    >
      <Wand2 className="w-4 h-4" />
      <span className="font-medium">Auto</span>
      {autoSelect && <Check className="w-3 h-3 ml-auto" />}
    </button>
  );

  // ── Transition badge ───────────────────────────────────────────────

  const transitionBadge = transitioning && (
    <div className="flex items-center gap-2 text-xs text-blue-400 animate-pulse">
      <div className="w-3 h-3 border-2 border-blue-400 border-t-transparent rounded-full animate-spin" />
      Transitioning...
    </div>
  );

  // ── Compact mode ───────────────────────────────────────────────────

  if (compact) {
    return (
      <div className={cn('space-y-2', className)}>
        <div className="flex flex-wrap gap-2">
          {autoToggle}
          {FORCE_PRESETS.map((preset) => {
            const Icon = preset.icon;
            const isActive = activeId === preset.id;
            return (
              <button
                key={preset.id}
                type="button"
                onClick={() => selectPreset(preset.id)}
                disabled={transitioning}
                className={cn(
                  'flex items-center gap-2 px-3 py-2 rounded-lg border transition-all',
                  'bg-white/5 hover:bg-white/10 border-white/10',
                  isActive && 'ring-2 ring-blue-500 border-blue-500/50 bg-blue-500/10',
                  transitioning && !isActive && 'opacity-50 cursor-not-allowed',
                )}
                title={preset.description}
              >
                <Icon className="w-4 h-4" />
                <span className="text-sm font-medium">{preset.label.split(' ')[0]}</span>
                {isActive && <Check className="w-3 h-3 ml-1 text-blue-400" />}
              </button>
            );
          })}
        </div>
        {transitionBadge}
      </div>
    );
  }

  // ── Full grid mode ─────────────────────────────────────────────────

  return (
    <div className={cn('space-y-3', className)}>
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold text-gray-200">Force Preset</h3>
        <div className="flex items-center gap-3">
          {transitionBadge}
          {autoToggle}
        </div>
      </div>

      <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-5 gap-3">
        {FORCE_PRESETS.map((preset) => {
          const Icon = preset.icon;
          const isActive = activeId === preset.id;

          return (
            <button
              key={preset.id}
              type="button"
              onClick={() => selectPreset(preset.id)}
              disabled={transitioning}
              className={cn(
                'relative flex flex-col items-center gap-2 p-4 rounded-xl border transition-all',
                'bg-white/5 hover:bg-white/10 border-white/10 hover:border-white/20',
                isActive && 'ring-2 ring-blue-500 border-blue-500/50 bg-blue-500/10 shadow-lg shadow-blue-500/10',
                transitioning && !isActive && 'opacity-50 cursor-not-allowed',
              )}
            >
              <Icon className={cn('w-6 h-6', isActive ? 'text-blue-400' : 'text-gray-400')} />
              <span className={cn('text-sm font-medium', isActive ? 'text-blue-300' : 'text-gray-300')}>
                {preset.label}
              </span>
              <span className="text-[11px] text-gray-500">{preset.nodeCountHint}</span>
              {isActive && (
                <div className="absolute top-2 right-2">
                  <Check className="w-3.5 h-3.5 text-blue-400" />
                </div>
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
};

export default ForcePresetSelector;

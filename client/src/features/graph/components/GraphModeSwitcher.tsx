import React, { useState, useCallback, useEffect } from 'react';
import { Box, Square, Glasses, Check, AlertTriangle } from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import { cn } from '../../../utils/classNameUtils';
import { createLogger } from '../../../utils/loggerConfig';

const logger = createLogger('GraphModeSwitcher');

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type GraphViewMode = '3d' | '2d' | 'vr';

interface ModeDefinition {
  id: GraphViewMode;
  label: string;
  icon: LucideIcon;
  description: string;
  /** Optional note shown below the mode bar when this mode is active. */
  note?: string;
  /** When set, mode availability is gated on this async check. */
  capabilityCheck?: () => Promise<boolean>;
}

export interface GraphModeSwitcherProps {
  /** Currently active mode. When omitted, component manages its own state. */
  activeMode?: GraphViewMode;
  /** Fired when the user selects a different view mode. */
  onModeChange: (mode: GraphViewMode) => void;
  className?: string;
}

// ---------------------------------------------------------------------------
// Mode definitions
// ---------------------------------------------------------------------------

const MODES: ModeDefinition[] = [
  {
    id: '3d',
    label: '3D',
    icon: Box,
    description: 'Full Three.js/R3F rendering with post-processing effects.',
  },
  {
    id: '2d',
    label: '2D',
    icon: Square,
    description: 'Canvas-based 2D force layout.',
    note: 'Lower GPU usage, better for large graphs',
  },
  {
    id: 'vr',
    label: 'VR',
    icon: Glasses,
    description: 'WebXR immersive mode.',
    note: 'Requires WebXR-compatible headset',
    capabilityCheck: async () => {
      if (!navigator.xr) return false;
      try {
        return await navigator.xr.isSessionSupported('immersive-vr');
      } catch {
        return false;
      }
    },
  },
];

const MODE_COLORS: Record<GraphViewMode, string> = {
  '3d': 'bg-blue-500/10   hover:bg-blue-500/20  border-blue-500/30',
  '2d': 'bg-emerald-500/10 hover:bg-emerald-500/20 border-emerald-500/30',
  'vr': 'bg-purple-500/10 hover:bg-purple-500/20 border-purple-500/30',
};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const GraphModeSwitcher: React.FC<GraphModeSwitcherProps> = ({
  activeMode: controlledMode,
  onModeChange,
  className,
}) => {
  const [internalMode, setInternalMode] = useState<GraphViewMode>('3d');
  const [vrSupported, setVrSupported] = useState<boolean | null>(null);

  const activeMode = controlledMode ?? internalMode;

  // Probe WebXR support once on mount
  useEffect(() => {
    const vrDef = MODES.find((m) => m.id === 'vr');
    if (vrDef?.capabilityCheck) {
      vrDef.capabilityCheck().then(setVrSupported).catch(() => setVrSupported(false));
    }
  }, []);

  const handleSelect = useCallback(
    (mode: GraphViewMode) => {
      if (mode === 'vr' && vrSupported === false) {
        logger.warn('WebXR immersive-vr not supported on this device');
        return;
      }
      setInternalMode(mode);
      onModeChange(mode);
    },
    [onModeChange, vrSupported],
  );

  const activeDef = MODES.find((m) => m.id === activeMode);

  return (
    <div className={cn('flex flex-col gap-1.5', className)}>
      {/* Mode button bar */}
      <div className="flex gap-2">
        {MODES.map((mode) => {
          const Icon = mode.icon;
          const isActive = activeMode === mode.id;
          const isDisabled = mode.id === 'vr' && vrSupported === false;

          return (
            <button
              key={mode.id}
              type="button"
              onClick={() => handleSelect(mode.id)}
              disabled={isDisabled}
              title={mode.description}
              className={cn(
                'relative flex items-center gap-2 px-3 py-2 rounded-lg border transition-all',
                MODE_COLORS[mode.id],
                isActive && 'ring-2 ring-primary shadow-md',
                isDisabled && 'opacity-40 cursor-not-allowed',
              )}
            >
              <Icon className="w-4 h-4 shrink-0" />
              <span className="text-sm font-medium">{mode.label}</span>
              {isActive && <Check className="w-3 h-3 ml-0.5 shrink-0" />}
            </button>
          );
        })}
      </div>

      {/* Contextual note for the active mode */}
      {activeDef?.note && (
        <div className="flex items-center gap-1.5 text-xs text-gray-500 dark:text-gray-400 pl-1">
          <AlertTriangle className="w-3 h-3 shrink-0" />
          <span>{activeDef.note}</span>
        </div>
      )}
    </div>
  );
};

export default GraphModeSwitcher;

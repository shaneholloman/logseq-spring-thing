/**
 * Visibility Control (ADR-049, Sprint 3).
 *
 * Renders the Private <-> Public toggle on a KG node:
 *   - Owner sees an interactive switch guarded by a confirmation modal and a
 *     POST to `/api/nodes/{id}/publish` or `/unpublish`.
 *   - Non-owner sees a read-only "Private (owner: npub1...)" chip with
 *     opacified styling.
 *
 * Gated by feature flag `VISIBILITY_TRANSITIONS`. Falls back to a plain
 * "Public" chip if disabled.
 */

import React, { useMemo, useState } from 'react';
import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../design-system/components';
import { useVisibilityStore } from '../graph/store/visibilitySlice';
import { useFeatureFlag } from '../../services/featureFlags';
import type { KGNode, NodeVisibility } from '../graph/types/graphTypes';
import { nostrAuth } from '../../services/nostrAuthService';

export interface VisibilityControlProps {
  node: KGNode;
  /** When provided, short-circuit owner detection (useful in tests). */
  forceOwner?: boolean;
  className?: string;
}

function truncateNpub(npub: string): string {
  if (npub.length <= 14) return npub;
  return `${npub.slice(0, 10)}...${npub.slice(-4)}`;
}

function resolveEffectiveVisibility(
  node: KGNode,
  overlay: NodeVisibility | undefined,
): NodeVisibility {
  return overlay ?? node.visibility ?? 'public';
}

export function VisibilityControl({
  node,
  forceOwner,
  className,
}: VisibilityControlProps): React.ReactElement | null {
  const enabled = useFeatureFlag('VISIBILITY_TRANSITIONS');
  const overlayRecord = useVisibilityStore((s) => s.overlay[node.id]);
  const busy = useVisibilityStore((s) => s.busyId === node.id);
  const error = useVisibilityStore((s) => s.error);
  const publish = useVisibilityStore((s) => s.publish);
  const unpublish = useVisibilityStore((s) => s.unpublish);
  const clearError = useVisibilityStore((s) => s.clearError);

  const [confirmOpen, setConfirmOpen] = useState(false);

  const visibility = resolveEffectiveVisibility(node, overlayRecord?.visibility);
  const ownerPubkey = overlayRecord?.owner_pubkey ?? node.owner_pubkey;
  const podUrl = overlayRecord?.pod_url ?? node.pod_url;

  const isOwner = useMemo(() => {
    if (typeof forceOwner === 'boolean') return forceOwner;
    if (!ownerPubkey) return false;
    const me = nostrAuth.getCurrentUser()?.pubkey;
    return !!me && me.toLowerCase() === ownerPubkey.toLowerCase();
  }, [forceOwner, ownerPubkey]);

  const ownerNpub = useMemo(() => {
    if (!ownerPubkey) return null;
    return nostrAuth.hexToNpub(ownerPubkey) ?? ownerPubkey;
  }, [ownerPubkey]);

  // Feature disabled: show a minimal public badge so the UI does not go blank.
  if (!enabled) {
    return (
      <Badge variant="outline" className={className}>
        Public
      </Badge>
    );
  }

  // Tombstone - show read-only decayed state.
  if (visibility === 'tombstone') {
    return (
      <Badge
        variant="destructive"
        className={className}
        title="This node was just unpublished"
      >
        Tombstoned
      </Badge>
    );
  }

  // Non-owner view.
  if (!isOwner) {
    if (visibility === 'private') {
      return (
        <Badge
          variant="outline"
          className={`opacity-60 ${className ?? ''}`}
          title={ownerNpub ?? 'Private node'}
        >
          Private {ownerNpub ? `(owner: ${truncateNpub(ownerNpub)})` : ''}
        </Badge>
      );
    }
    return (
      <Badge variant="outline" className={className}>
        Public
      </Badge>
    );
  }

  // Owner view - interactive toggle.
  const nextVisibility: NodeVisibility = visibility === 'public' ? 'private' : 'public';
  const actionLabel = visibility === 'public' ? 'Make private' : 'Publish';
  const modalTitle =
    visibility === 'public' ? 'Unpublish node?' : 'Publish node to Solid pod?';
  const modalBody =
    visibility === 'public'
      ? 'The node will be marked private and the Solid pod record will be torn down. Viewers without your pubkey will see an opaque placeholder.'
      : 'The node metadata will be written to your Solid pod and broadcast on the migration event stream. Other participants will be able to cite its URN.';

  const handleConfirm = async (): Promise<void> => {
    if (visibility === 'public') {
      await unpublish(node.id);
    } else {
      await publish(node.id);
    }
    setConfirmOpen(false);
  };

  return (
    <div className={`flex items-center gap-2 ${className ?? ''}`}>
      <Badge
        variant={visibility === 'public' ? 'outline' : 'secondary'}
        className={visibility === 'private' ? 'opacity-80' : ''}
      >
        {visibility === 'public' ? 'Public' : 'Private'}
      </Badge>
      {podUrl && visibility === 'public' && (
        <a
          href={podUrl}
          target="_blank"
          rel="noreferrer noopener"
          className="text-[10px] text-cyan-300 underline truncate max-w-[160px]"
          title={podUrl}
        >
          pod
        </a>
      )}
      <Button
        variant="ghost"
        size="sm"
        disabled={busy}
        onClick={() => setConfirmOpen(true)}
        aria-label={actionLabel}
      >
        {busy ? 'Working...' : actionLabel}
      </Button>
      {error && (
        <span className="text-[10px] text-red-400" onClick={clearError} role="alert">
          {error} (dismiss)
        </span>
      )}

      <Dialog open={confirmOpen} onOpenChange={setConfirmOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{modalTitle}</DialogTitle>
            <DialogDescription>{modalBody}</DialogDescription>
          </DialogHeader>
          <div className="text-xs text-muted-foreground">
            Node: <code className="text-foreground">{node.id}</code>
            {' -> '}
            <span className="font-medium">{nextVisibility}</span>
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setConfirmOpen(false)}
              disabled={busy}
            >
              Cancel
            </Button>
            <Button
              variant={visibility === 'public' ? 'destructive' : 'default'}
              onClick={() => void handleConfirm()}
              disabled={busy}
              loading={busy}
            >
              {visibility === 'public' ? 'Unpublish' : 'Publish'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

export default VisibilityControl;

import { createLogger, createErrorMetadata } from '../../utils/loggerConfig'

const logger = createLogger('SettingsStore')

// --- Subscriber trie (O(depth) prefix matching) ---
export type SubscriberCallback = () => void;

interface SubscriberTrieNode {
  subscribers: Set<SubscriberCallback>;
  children: Map<string, SubscriberTrieNode>;
}

export const subscriberTrieRoot: SubscriberTrieNode = {
  subscribers: new Set(),
  children: new Map(),
};

export function getOrCreateTrieNode(path: string, create = true): SubscriberTrieNode | undefined {
  if (!path) return subscriberTrieRoot;
  const segments = path.split('.');
  let node = subscriberTrieRoot;
  for (const segment of segments) {
    let next = node.children.get(segment);
    if (!next && create) {
      next = { subscribers: new Set(), children: new Map() };
      node.children.set(segment, next);
    }
    if (!next) return undefined;
    node = next;
  }
  return node;
}

export function collectDescendants(node: SubscriberTrieNode, out: Set<SubscriberCallback>): void {
  for (const cb of node.subscribers) out.add(cb);
  for (const child of node.children.values()) {
    collectDescendants(child, out);
  }
}

export function collectMatchedCallbacks(changedPaths: string[]): Set<SubscriberCallback> {
  const result = new Set<SubscriberCallback>();
  for (const path of changedPaths) {
    const segments = path.split('.');
    let node = subscriberTrieRoot;
    // Walk down: collect all ancestor subscribers (prefix match)
    for (let i = 0; i <= segments.length; i++) {
      if (node.subscribers.size) {
        for (const cb of node.subscribers) result.add(cb);
      }
      if (i === segments.length) break;
      const next = node.children.get(segments[i]);
      if (!next) break;
      node = next;
    }
    // Also collect all descendant subscribers from the exact node
    const exactNode = getOrCreateTrieNode(path, false);
    if (exactNode) collectDescendants(exactNode, result);
  }
  return result;
}

// --- RAF-batched subscriber notification ---
export let pendingNotifyCallbacks = new Set<SubscriberCallback>();
export let notifyRafScheduled = false;

export function flushNotifyCallbacks(): void {
  const callbacks = pendingNotifyCallbacks;
  pendingNotifyCallbacks = new Set();
  notifyRafScheduled = false;
  for (const cb of callbacks) {
    try { cb(); } catch (error) {
      logger.error('Error in settings subscriber during updateSettings:', createErrorMetadata(error));
    }
  }
}

export function scheduleNotify(matchedCallbacks: Set<SubscriberCallback>): void {
  for (const cb of matchedCallbacks) pendingNotifyCallbacks.add(cb);
  if (!notifyRafScheduled) {
    notifyRafScheduled = true;
    requestAnimationFrame(flushNotifyCallbacks);
  }
}

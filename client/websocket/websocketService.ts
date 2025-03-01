import { createLogger, createErrorMetadata, createMessageMetadata, createDataMetadata } from '../core/logger';
import { buildWsUrl } from '../core/api';
import { debugState } from '../core/debugState';
import { Vector3 } from 'three';
import { createVector3, zeroVector3, vector3ToObject, isValidVector3, clampVector3, vector3Equals } from '../utils/vectorUtils';
import pako from 'pako';

const logger = createLogger('WebSocketService');

// Throttle for debug logging to prevent excessive logs
let lastDebugLogTime = 0;
const DEBUG_LOG_THROTTLE_MS = 1000; // Only log once per second

// Helper for conditional debug logging
function debugLog(message: string, ...args: any[]) {
    if (debugState.isWebsocketDebugEnabled()) {
        const now = Date.now();
        if (now - lastDebugLogTime > DEBUG_LOG_THROTTLE_MS) {
            lastDebugLogTime = now;
            logger.debug(message, ...args);
        }
    }
}

// Compression settings
const COMPRESSION_THRESHOLD = 1024; // Only compress messages larger than 1KB

enum ConnectionState {
    DISCONNECTED = 'disconnected',
    CONNECTING = 'connecting',
    CONNECTED = 'connected',
    RECONNECTING = 'reconnecting',
    FAILED = 'failed'
}

// Interface for node updates from user interaction
interface NodeUpdate {
    id: string;          // Node ID (string in metadata, but must be converted to u32 index for binary protocol)
    position: Vector3;   // Current position (Three.js Vector3)
    velocity?: Vector3;  // Optional velocity (Three.js Vector3)
    metadata?: {
        name?: string;
        lastModified?: number;
        links?: string[];
        references?: string[];
        fileSize?: number;
        hyperlinkCount?: number;
    };
}

// Interface matching server's binary protocol format (28 bytes per node):
// - id: 4 bytes (u32)
// - position: 12 bytes (Vec3Data)
// - velocity: 12 bytes (Vec3Data)
interface BinaryNodeData {
    id: number;
    position: Vector3;   // Three.js Vector3
    velocity: Vector3;   // Three.js Vector3
}

type BinaryMessageCallback = (nodes: BinaryNodeData[]) => void;

export class WebSocketService {
    private static instance: WebSocketService | null = null;
    private ws: WebSocket | null = null;
    private binaryMessageCallback: BinaryMessageCallback | null = null;
    private reconnectTimeout: number | null = null;
    private connectionState: ConnectionState = ConnectionState.DISCONNECTED;
    private reconnectAttempts: number = 0;
    private readonly _maxReconnectAttempts: number = 5;
    // Keep track of node ID to numeric index mapping for binary protocol
    private nodeNameToIndexMap: Map<string, number> = new Map();
    private nextNodeIndex: number = 0;
    private readonly initialReconnectDelay: number = 1000; // 1 second (reduced from 5000)
    private readonly maxReconnectDelay: number = 60000; // 60 seconds
    private url: string = '';
    private connectionStatusHandler: ((status: boolean) => void) | null = null;
    private readonly MAX_POSITION = 1000.0;
    private readonly MAX_VELOCITY = 0.05; // Reduced to align with server's MAX_VELOCITY (0.02)

    // Add a debounce mechanism for node updates
    private loadingStatusHandler: ((isLoading: boolean, message?: string) => void) | null = null;
    private isLoading: boolean = false;
    private nodeUpdateQueue: NodeUpdate[] = [];
    private nodeUpdateTimer: number | null = null;
    private readonly NODE_UPDATE_DEBOUNCE_MS = 50; // 50ms debounce for node updates

    private validateAndClampVector3(vec: Vector3, max: number): Vector3 {
        if (!isValidVector3(vec)) {
            // Return a valid vector at origin rather than zeroing out
            return zeroVector3();
        }
        
        // If the vector has NaN or infinite values, replace with zero
        const sanitizedVec = new Vector3(
            isNaN(vec.x) || !isFinite(vec.x) ? 0 : vec.x,
            isNaN(vec.y) || !isFinite(vec.y) ? 0 : vec.y,
            isNaN(vec.z) || !isFinite(vec.z) ? 0 : vec.z
        );
        
        return clampVector3(sanitizedVec, -max, max);
    }

    private constructor() {
        // Don't automatically connect - wait for explicit connect() call
        
        // Listen for reset events (when graph data is cleared)
        window.addEventListener('graph-data-reset', () => {
            this.resetNodeIndices();
        });
    }
    
    private resetNodeIndices(): void {
        this.nodeNameToIndexMap.clear();
        this.nextNodeIndex = 0;
    }

    public static getInstance(): WebSocketService {
        if (!WebSocketService.instance) {
            WebSocketService.instance = new WebSocketService();
        }
        return WebSocketService.instance;
    }

    public connect(): Promise<void> {
        if (this.connectionState !== ConnectionState.DISCONNECTED) {
            // Only log this at debug level instead of warn to reduce log spam
            if (this.connectionState === ConnectionState.CONNECTED) {
                logger.info('WebSocket already connected');
                return Promise.resolve();
            }
            
            logger.info(`WebSocket in ${this.connectionState} state, attempting to reconnect...`);
            
            // If in FAILED state, reset and try again
            if (this.connectionState === ConnectionState.FAILED) {
                this.connectionState = ConnectionState.DISCONNECTED;
                this.reconnectAttempts = 0;
                return this.initializeWebSocket();
            }
            
            // If already connecting, return a promise that resolves when connected
            if (this.connectionState === ConnectionState.CONNECTING) {
                return new Promise((resolve) => {
                    const checkConnection = () => {
                        if (this.connectionState === ConnectionState.CONNECTED) {
                            resolve();
                        } else {
                            setTimeout(checkConnection, 100);
                        }
                    };
                    checkConnection();
                });
            }
            return Promise.resolve();
        }
        return this.initializeWebSocket();
    }

    private async initializeWebSocket(): Promise<void> {
        if (this.connectionState !== ConnectionState.DISCONNECTED) {
            return;
        }

        try {
            this.url = buildWsUrl();
            
            if (!this.url) {
                throw new Error('No WebSocket URL available');
            }

            this.connectionState = ConnectionState.CONNECTING;
            return new Promise((resolve, reject) => {
                this.ws = new WebSocket(this.url);
                this.setupWebSocketHandlers();
                
                // Add one-time open handler to resolve the promise
                this.ws!.addEventListener('open', () => resolve(), { once: true });
                // Add one-time error handler to reject the promise
                this.ws!.addEventListener('error', (e) => reject(e), { once: true });
            });
        } catch (error) {
            logger.error('Failed to initialize WebSocket:', createErrorMetadata(error));
            this.handleReconnect();
            return Promise.reject(error);
        }
    }

    private getReconnectDelay(): number {
        // Exponential backoff with max delay
        const delay = Math.min(
            this.initialReconnectDelay * Math.pow(2, this.reconnectAttempts),
            this.maxReconnectDelay
        );
        // Add some jitter
        return delay + (Math.random() * 1000);
    }

    private setupWebSocketHandlers(): void {
        if (!this.ws) return;
        
        this.ws.binaryType = 'arraybuffer';

        this.ws.onopen = (): void => {
            logger.info('WebSocket connected successfully to', createMessageMetadata(this.url));
            this.connectionState = ConnectionState.CONNECTED;
            this.reconnectAttempts = 0;

            if (this.connectionStatusHandler) {
                this.connectionStatusHandler(true);
                debugLog('Connection status handler notified: connected');
            }
            logger.info('WebSocket connected successfully, requesting initial position data');

            // Send request for position updates after connection
            debugLog('Requesting position updates');
            this.sendMessage({ type: 'requestInitialData' }); // Matching the server's camelCase type

            // Randomization is disabled by default until client is ready
            logger.info('WebSocket connection established. Randomization disabled by default.');
        };

        this.ws.onerror = (event: Event): void => {
            logger.error('WebSocket error:', createDataMetadata(event));
            // Don't call handleReconnect here, let onclose handle it
            // This prevents duplicate reconnection attempts when both error and close events fire
            // if (this.ws?.readyState === WebSocket.CLOSED) {
            //     this.handleReconnect();
            // }
        };

        this.ws.onclose = (event: CloseEvent): void => {
            logger.warn('WebSocket closed', createDataMetadata({
                code: event.code,
                reason: event.reason
            }));
            
            if (this.connectionStatusHandler) {
                this.connectionStatusHandler(false);
            }
            
            this.handleReconnect();
        };

        this.ws.onmessage = (event: MessageEvent) => {
            try {
                if (event.data instanceof ArrayBuffer) {
                    
                    this.handleBinaryMessage(event.data);
                } else if (typeof event.data === 'string') {
                    try {
                        const message = JSON.parse(event.data);
                        
                        // Handle loading state messages
                        if (message.type === 'loading') {
                            this.isLoading = true;
                            logger.info('WebSocket loading state:', createDataMetadata({
                                message: message.message
                            }));
                            // Notify loading handler if registered
                            if (this.loadingStatusHandler) {
                                this.loadingStatusHandler(true, message.message);
                            }
                        } else if (message.type === 'updatesStarted') {
                            // Clear loading state when updates start
                            this.isLoading = false;
                            logger.info('WebSocket updates started:', createDataMetadata({
                                timestamp: message.timestamp
                            }));
                            // Notify loading handler if registered
                            if (this.loadingStatusHandler) {
                                this.loadingStatusHandler(false);
                            }
                        } else if (message.type === 'connection_established') {
                            logger.info('WebSocket message received:', createDataMetadata({
                                type: message.type,
                                timestamp: message.timestamp || Date.now()
                            }));
                        } else if (debugState.isWebsocketDebugEnabled()) {
                            logger.debug('WebSocket message received:', message);
                        }
                    } catch (error) {
                        logger.error('Failed to parse WebSocket message:', createErrorMetadata(error));
                    }
                }
            } catch (error) {
                logger.error('Critical error in message handler:', createErrorMetadata(error));
            }
        };
    }

    private tryDecompress(buffer: ArrayBuffer): ArrayBuffer {
        try {
            const decompressed = pako.inflate(new Uint8Array(buffer));
            if (decompressed.length < 8 || decompressed.length % 4 !== 0) {
                return buffer;
            }
            return decompressed.buffer;
        } catch (error) {
            return buffer;
        }
    }

    private compressIfNeeded(buffer: ArrayBuffer): ArrayBuffer {
        if (buffer.byteLength > COMPRESSION_THRESHOLD) {
            try {
                const compressed = pako.deflate(new Uint8Array(buffer));
                return compressed.buffer;
            } catch (error) {
                logger.warn('Compression failed, using original data:', createErrorMetadata(error));
                return buffer;
            }
        }
        return buffer;
    }

    private handleBinaryMessage(buffer: ArrayBuffer): void {
        try {
            // Log raw buffer details before processing
            const isCompressed = buffer.byteLength > 0 && buffer.byteLength % 28 !== 0;

            const decompressedBuffer = this.tryDecompress(buffer);
           
            // Throttled debug logging for binary messages
             debugLog('Binary data processed:', createDataMetadata({ 
                rawSize: buffer.byteLength, 
                decompressedSize: decompressedBuffer.byteLength, 
                isCompressed,
                nodeCount: decompressedBuffer.byteLength / 28
            }));
            
            if (!decompressedBuffer || decompressedBuffer.byteLength % 28 !== 0) {
                // Enhanced error logging for production debugging
                const errorDetails = {
                    bufferSize: buffer.byteLength,
                    decompressedSize: decompressedBuffer?.byteLength ?? 0,
                    remainder: (decompressedBuffer?.byteLength ?? 0) % 28,
                    expectedNodeCount: Math.floor((decompressedBuffer?.byteLength ?? 0) / 28),
                    url: this.url
                };
                logger.error('Invalid binary message size:', createDataMetadata(errorDetails));
                throw new Error(`Invalid buffer size: ${decompressedBuffer?.byteLength ?? 0} bytes (not a multiple of 28)`);
            }

            const dataView = new DataView(decompressedBuffer);
            const nodeCount = decompressedBuffer.byteLength / 28;

            if (nodeCount === 0) {
                debugLog('No nodes in binary update');
            }
            let offset = 0;
            let invalidValuesFound = false;
            const nodes: BinaryNodeData[] = [];
            
            for (let i = 0; i < nodeCount; i++) {
                const id = dataView.getUint32(offset, true);
                offset += 4;

                const position = createVector3(
                    dataView.getFloat32(offset, true),      // x
                    dataView.getFloat32(offset + 4, true),  // y
                    dataView.getFloat32(offset + 8, true)   // z
                );
                offset += 12;

                const velocity = createVector3(
                    dataView.getFloat32(offset, true),      // x
                    dataView.getFloat32(offset + 4, true),  // y
                    dataView.getFloat32(offset + 8, true)   // z
                );
                offset += 12;
                
                // Validate and clamp position and velocity
                // No longer being lenient with position validation to prevent node explosions
                const sanitizedPosition = this.validateAndClampVector3(position, this.MAX_POSITION);
                const sanitizedVelocity = this.validateAndClampVector3(velocity, this.MAX_VELOCITY);
                
                // Check if values were invalid using vector3Equals
                if (!vector3Equals(position, sanitizedPosition) || !vector3Equals(velocity, sanitizedVelocity)) {
                    invalidValuesFound = true;
                    logger.warn('Invalid values detected in binary message:', createDataMetadata({
                        nodeId: id,
                        originalPosition: vector3ToObject(position),
                        sanitizedPosition: vector3ToObject(sanitizedPosition),
                        originalVelocity: vector3ToObject(velocity),
                        sanitizedVelocity: vector3ToObject(sanitizedVelocity)
                    }));
                }

                nodes.push({ id, position: sanitizedPosition, velocity: sanitizedVelocity });
            }

            if (invalidValuesFound) {
                logger.warn('Some nodes had invalid position/velocity values that were clamped');
            }

            if (nodes.length > 0 && this.binaryMessageCallback) {
                this.binaryMessageCallback(nodes);  // Send to NodeManagerFacade
            }
        } catch (error) {
            logger.error('Failed to process binary message:', createErrorMetadata(error));
        }
    }

    private handleReconnect(): void {
        const wasConnected = this.connectionState === ConnectionState.CONNECTED;
        
        this.connectionState = ConnectionState.DISCONNECTED;
        this.binaryMessageCallback = null;
        
        if (this.reconnectTimeout !== null) {
            window.clearTimeout(this.reconnectTimeout);
            this.reconnectTimeout = null;
        }
        
        if (this.reconnectAttempts < this._maxReconnectAttempts &&
            (wasConnected || this.reconnectAttempts === 0)) {
            
            this.reconnectAttempts++;
            const delay = this.getReconnectDelay();
            
            this.connectionState = ConnectionState.RECONNECTING;
            
            this.reconnectTimeout = window.setTimeout(async () => {
                this.reconnectTimeout = null;
                try {
                    await this.connect();
                } catch (error) {
                    logger.error('Reconnection attempt failed:', createErrorMetadata(error));
                }
            }, delay);
        } else {
            this.handleReconnectFailure();
        }
    }

    private handleReconnectFailure(): void {
        this.connectionState = ConnectionState.FAILED;
        if (this.connectionStatusHandler) {
            this.connectionStatusHandler(false);
        }
    }

    public onBinaryMessage(callback: BinaryMessageCallback): void {
        this.binaryMessageCallback = callback;
    }

    public getConnectionStatus(): ConnectionState {
        return this.connectionState;
    }

    public sendMessage(message: any): void {
        if (this.ws && this.ws.readyState === WebSocket.OPEN) {
            try {
                this.ws.send(JSON.stringify(message));
            } catch (error) {
                logger.error('Error sending message:', createErrorMetadata(error));
            }
        }
    }

    /**
     * Enable or disable server-side node position randomization
     * This should only be called after the initial data loading is complete
     * @param enabled Whether randomization should be enabled
     */
    public enableRandomization(enabled: boolean): void {
        if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
            if (enabled) {
                logger.warn('WebSocket not connected, attempting to reconnect before enabling randomization');
                // Try to reconnect
                this.connect().then(() => {
                    // If connection succeeded, try again
                    if (this.ws?.readyState === WebSocket.OPEN) {
                        logger.info(`${enabled ? 'Enabling' : 'Disabling'} server-side position randomization after reconnection`);
                        this.sendMessage({ type: 'enableRandomization', enabled });
                    }
                }).catch(e => {
                    logger.error('Failed to reconnect for randomization:', createErrorMetadata(e));
                });
            } else {
                logger.warn('WebSocket not connected, cannot disable randomization');
            }
            return;
        }

        logger.info(`${enabled ? 'Enabling' : 'Disabling'} server-side position randomization`);
        this.sendMessage({ type: 'enableRandomization', enabled });
    }
    
    
    public sendNodeUpdates(updates: NodeUpdate[]): void {
        if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
            logger.warn('WebSocket not connected, attempting to reconnect before sending updates');
            // Try to reconnect and then send updates 
            this.connect().then(() => {
                // Check if connection succeeded
                if (this.ws?.readyState === WebSocket.OPEN) {
                    logger.info('Reconnected successfully, now sending queued node updates');
                    this.nodeUpdateQueue.push(...updates);
                    this.processNodeUpdateQueue();
                }
            }).catch(e => {
                logger.error('Failed to reconnect for node updates:', createErrorMetadata(e));
            });
            return;
        }

        // Pre-validate node IDs before adding to queue
        const validatedUpdates = updates.filter(update => {
            const id = parseInt(update.id, 10);
            
            // Check for NaN or non-numeric IDs
            if (isNaN(id) || id < 0 || !Number.isInteger(id)) {
                // This is likely a metadata name being incorrectly used as a node ID
                logger.warn('Invalid node ID:', createDataMetadata({
                    message: update.id,
                    valueType: typeof update.id,
                    invalidReason: isNaN(id) ? 'Not a numeric ID' : 
                                  id < 0 ? 'Negative ID not allowed' :
                                  'Non-integer ID not allowed',
                    attemptedParse: id
                }));
                
                // Log additional context to help diagnose the issue
                if (typeof update.id === 'string' && update.id.length > 10) {
                    logger.warn('Possible metadata name detected as ID', createDataMetadata({ id: update.id }));
                }
                return false;
            }
            return true;
        });

        if (validatedUpdates.length === 0 && updates.length > 0) {
            // If we have non-numeric node IDs (metadata names), convert them to numeric indices
            const indexedUpdates = updates.map(update => {
                // Get or create index for this metadata name
                if (!this.nodeNameToIndexMap.has(update.id)) {
                    // Assign a new numeric index to this metadata name
                    this.nodeNameToIndexMap.set(update.id, this.nextNodeIndex++);
                    logger.info(`Mapped metadata name "${update.id}" to numeric index ${this.nextNodeIndex-1} for binary protocol`);
                }
                
                // Use the numeric index for the binary protocol
                const numericId = this.nodeNameToIndexMap.get(update.id)!;
                return { ...update, id: numericId.toString() };
            });
            
            // Add the indexed updates to the queue
            this.nodeUpdateQueue.push(...indexedUpdates);
        } else if (validatedUpdates.length > 0) {
            // Add already-numeric updates to the queue
            this.nodeUpdateQueue.push(...validatedUpdates);
        } else {
            return; // No updates to process
        }
        
        
        // Debounce updates to prevent flooding the server
        if (this.nodeUpdateTimer === null) {
            this.nodeUpdateTimer = window.setTimeout(() => {
                this.processNodeUpdateQueue();
                this.nodeUpdateTimer = null;
            }, this.NODE_UPDATE_DEBOUNCE_MS);
        }
    }
    
    private processNodeUpdateQueue(): void {
        if (!this.ws || this.ws.readyState !== WebSocket.OPEN || this.nodeUpdateQueue.length === 0) {
            this.nodeUpdateQueue = [];
            return;
        }
        
        // Get the most recent updates for each node ID (to avoid sending outdated positions)
        const latestUpdates = new Map<string, NodeUpdate>();
        for (const update of this.nodeUpdateQueue) {
            latestUpdates.set(update.id, update);
        }
        
        // Convert to array and limit to 2 nodes per update as per server requirements
        let updates = Array.from(latestUpdates.values());
        if (updates.length > 2) {
            debugLog('Too many nodes in update, limiting to first 2');
            updates = updates.slice(0, 2);
        }
        
        // Clear the queue
        this.nodeUpdateQueue = [];

        const buffer = new ArrayBuffer(updates.length * 28);
        const dataView = new DataView(buffer);
        let offset = 0;

        updates.forEach(update => {
            const id = parseInt(update.id, 10); 
            if (isNaN(id)) {
                logger.warn('Invalid node ID in queue:', createDataMetadata({ 
                    nodeId: update.id,
                    type: typeof update.id,
                    length: typeof update.id === 'string' ? update.id.length : 0,
                    isPossibleMetadataName: typeof update.id === 'string' && 
                                          update.id.length > 10 && !/^\d+$/.test(update.id)
                }));
                return; // Skip this update
            }
            dataView.setUint32(offset, id, true);
            offset += 4;

            // Validate and clamp position
            const validPosition = this.validateAndClampVector3(update.position, this.MAX_POSITION);
            
            // Write position
            dataView.setFloat32(offset, validPosition.x, true);
            dataView.setFloat32(offset + 4, validPosition.y, true);
            dataView.setFloat32(offset + 8, validPosition.z, true);
            offset += 12;

            // Validate and clamp velocity (default to zero vector if not provided)
            const rawVelocity = update.velocity ?? zeroVector3();
            const validVelocity = this.validateAndClampVector3(rawVelocity, this.MAX_VELOCITY);
            
            // Write velocity
            dataView.setFloat32(offset, validVelocity.x, true);
            dataView.setFloat32(offset + 4, validVelocity.y, true);
            dataView.setFloat32(offset + 8, validVelocity.z, true);
            offset += 12;
        });

        const finalBuffer = this.compressIfNeeded(buffer);
        this.ws.send(finalBuffer);
    }

    public onConnectionStatusChange(handler: (status: boolean) => void): void {
        this.connectionStatusHandler = handler;
        if (this.connectionState === ConnectionState.CONNECTED && handler) {
            handler(true);
        }
    }

    /**
     * Register a handler for loading status changes
     * @param handler Callback function that receives loading state and optional message
     */
    public onLoadingStatusChange(handler: (isLoading: boolean, message?: string) => void): void {
        this.loadingStatusHandler = handler;
        
        // Immediately notify with current state if already loading
        if (this.isLoading && handler) {
            handler(true);
        }
    }

    public dispose(): void {
        if (this.reconnectTimeout !== null) {
            window.clearTimeout(this.reconnectTimeout);
            this.reconnectTimeout = null;
        }
        
        if (this.nodeUpdateTimer !== null) {
            window.clearTimeout(this.nodeUpdateTimer);
            this.nodeUpdateTimer = null;
            this.nodeUpdateQueue = [];
        }
        
        if (this.ws) {
            this.ws.close();
            this.ws = null;
        }
        
        this.binaryMessageCallback = null;
        this.connectionStatusHandler = null;
        this.connectionState = ConnectionState.DISCONNECTED;
        WebSocketService.instance = null;
    }

    public close(): void {
        if (this.ws) {
            this.ws.close();
            this.ws = null;
        }
    }
}
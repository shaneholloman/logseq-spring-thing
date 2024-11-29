import type {
  WebSocketConfig,
  WebSocketEventMap,
  WebSocketEventCallback,
  BaseMessage,
  ErrorMessage,
  GraphUpdateMessage,
  BinaryMessage,
  MessageType,
  SimulationModeMessage,
  SettingsUpdatedMessage,
  FisheyeUpdateMessage,
  RagflowResponse
} from '../types/websocket'

import {
  DEFAULT_RECONNECT_ATTEMPTS,
  DEFAULT_RECONNECT_DELAY,
  DEFAULT_MESSAGE_RATE_LIMIT,
  DEFAULT_MESSAGE_TIME_WINDOW,
  DEFAULT_MAX_MESSAGE_SIZE,
  DEFAULT_MAX_AUDIO_SIZE,
  DEFAULT_MAX_QUEUE_SIZE,
  CONNECTION_TIMEOUT,
  MESSAGE_TYPES,
  ERROR_CODES,
  ENABLE_BINARY_DEBUG
} from '../constants/websocket'

const DEFAULT_CONFIG: WebSocketConfig = {
  messageRateLimit: DEFAULT_MESSAGE_RATE_LIMIT,
  messageTimeWindow: DEFAULT_MESSAGE_TIME_WINDOW,
  maxMessageSize: DEFAULT_MAX_MESSAGE_SIZE,
  maxAudioSize: DEFAULT_MAX_AUDIO_SIZE,
  maxQueueSize: DEFAULT_MAX_QUEUE_SIZE,
  maxRetries: DEFAULT_RECONNECT_ATTEMPTS,
  retryDelay: DEFAULT_RECONNECT_DELAY
}

export default class WebsocketService {
  private ws: WebSocket | null = null;
  private config: WebSocketConfig;
  private messageQueue: any[] = [];
  private messageCount = 0;
  private lastMessageTime = 0;
  private reconnectAttempts = 0;
  private reconnectTimeout: number | null = null;
  private eventListeners: Map<keyof WebSocketEventMap, Set<WebSocketEventCallback<any>>> = new Map();
  private url: string;

  constructor(config: Partial<WebSocketConfig> = {}) {
    this.config = { ...DEFAULT_CONFIG, ...config };
    this.url = `${window.location.protocol === 'https:' ? 'wss:' : 'ws:'}//${window.location.host}/ws`;
    console.debug('WebSocket URL:', this.url);
  }

  public async connect(): Promise<void> {
    if (this.ws?.readyState === WebSocket.OPEN) {
      console.debug('WebSocket already connected');
      return;
    }

    return new Promise((resolve, reject) => {
      try {
        console.debug(`Attempting WebSocket connection (attempt ${this.reconnectAttempts + 1}/${this.config.maxRetries})...`);
        
        this.ws = new WebSocket(this.url);
        this.ws.binaryType = 'arraybuffer';

        const connectionTimeout = setTimeout(() => {
          if (this.ws?.readyState !== WebSocket.OPEN) {
            console.error('WebSocket connection timeout');
            this.ws?.close();
            reject(new Error(ERROR_CODES.CONNECTION_FAILED));
          }
        }, CONNECTION_TIMEOUT);

        this.ws.onopen = () => {
          clearTimeout(connectionTimeout);
          console.debug('WebSocket connection established');
          this.reconnectAttempts = 0;
          this.emit('open');
          this.processQueuedMessages();
          resolve();
        };

        this.ws.onclose = (event) => {
          clearTimeout(connectionTimeout);
          console.debug('WebSocket connection closed:', {
            code: event.code,
            reason: event.reason,
            wasClean: event.wasClean
          });
          this.handleConnectionClose(event.wasClean);
        };

        this.ws.onerror = (error) => {
          clearTimeout(connectionTimeout);
          console.error('WebSocket connection error:', error);
          const errorMsg: ErrorMessage = {
            type: MESSAGE_TYPES.ERROR,
            message: 'WebSocket connection error',
            code: ERROR_CODES.CONNECTION_FAILED
          };
          this.emit('error', errorMsg);
          reject(error);
        };

        this.ws.onmessage = this.handleMessage.bind(this);

      } catch (error) {
        console.error('Error creating WebSocket connection:', error);
        reject(error);
      }
    });
  }

  private handleMessage(event: MessageEvent): void {
    try {
      if (event.data instanceof ArrayBuffer) {
        if (ENABLE_BINARY_DEBUG) {
          console.debug('Received binary message:', {
            size: event.data.byteLength,
            timestamp: new Date().toISOString()
          });
        }

        // Handle binary position updates
        const dataView = new Float32Array(event.data);
        const isInitialLayout = dataView[0] >= 1.0;
        
        // Calculate node count from buffer size
        // Buffer contains: isInitialLayout(1) + (x,y,z,vx,vy,vz)(6) per node
        const totalFloats = dataView.length - 1; // Subtract isInitialLayout flag
        const nodeCount = totalFloats / 6; // 6 floats per node
        
        const binaryMessage: BinaryMessage = {
          data: event.data,
          isInitialLayout,
          nodeCount
        };
        
        this.emit('gpuPositions', binaryMessage);
        return;
      }

      // Handle JSON messages
      const message = JSON.parse(event.data) as BaseMessage;
      const type = message.type as MessageType;

      // Always emit the raw message
      this.emit('message', message);

      // Handle specific message types
      switch (type) {
        case MESSAGE_TYPES.GRAPH_UPDATE:
        case MESSAGE_TYPES.GRAPH_DATA:
          const graphMessage = message as GraphUpdateMessage;
          // Handle both camelCase and snake_case versions
          if (graphMessage.graphData || graphMessage.graph_data) {
            this.emit('graphUpdate', {
              type: graphMessage.type,
              graphData: graphMessage.graphData || graphMessage.graph_data
            });
          }
          break;

        case MESSAGE_TYPES.ERROR:
          const errorMessage = message as ErrorMessage;
          this.emit('error', {
            type: MESSAGE_TYPES.ERROR,
            message: errorMessage.message,
            details: errorMessage.details,
            code: errorMessage.code
          });
          break;

        case MESSAGE_TYPES.POSITION_UPDATE_COMPLETE:
          this.emit('positionUpdateComplete', message.status || 'complete');
          // Also emit the is_initial_layout flag if present
          if ('is_initial_layout' in message) {
            this.emit('message', {
              type: MESSAGE_TYPES.LAYOUT_STATE,
              isInitial: message.is_initial_layout
            });
          }
          break;

        case MESSAGE_TYPES.SIMULATION_MODE_SET:
          const simMessage = message as SimulationModeMessage & { gpu_enabled?: boolean };
          this.emit('simulationModeSet', simMessage.mode);
          // Also emit GPU state if present
          if ('gpu_enabled' in simMessage) {
            this.emit('message', {
              type: MESSAGE_TYPES.GPU_STATE,
              enabled: simMessage.gpu_enabled
            });
          }
          break;

        case MESSAGE_TYPES.SETTINGS_UPDATED:
          const settingsMessage = message as SettingsUpdatedMessage;
          this.emit('serverSettings', settingsMessage.settings);
          break;

        case MESSAGE_TYPES.FISHEYE_SETTINGS_UPDATED:
          const fisheyeMessage = message as FisheyeUpdateMessage & { focus_point?: [number, number, number] };
          // Handle both focus_point array and individual coordinates
          const focusPoint = fisheyeMessage.focus_point || (
            'fisheye_focus_x' in fisheyeMessage ? [
              (fisheyeMessage as any).fisheye_focus_x,
              (fisheyeMessage as any).fisheye_focus_y,
              (fisheyeMessage as any).fisheye_focus_z
            ] : [0, 0, 0]
          );
          
          this.emit('serverSettings', {
            fisheye: {
              enabled: fisheyeMessage.fisheye_enabled,
              strength: fisheyeMessage.fisheye_strength,
              focusPoint,
              radius: fisheyeMessage.fisheye_radius
            }
          });
          break;

        case 'ragflowResponse':
          const ragflowMessage = message as RagflowResponse;
          this.emit('ragflowAnswer', ragflowMessage.answer);
          break;

        case 'openaiResponse':
          this.emit('openaiResponse', message.response);
          break;

        case 'completion':
          this.emit('completion', message.text);
          break;

        default:
          console.debug('Unhandled message type:', type);
          break;
      }
    } catch (error) {
      console.error('Error handling WebSocket message:', error);
      const errorMsg: ErrorMessage = {
        type: MESSAGE_TYPES.ERROR,
        message: 'Error processing message',
        code: ERROR_CODES.INVALID_MESSAGE
      };
      this.emit('error', errorMsg);
    }
  }

  private handleConnectionClose(wasClean: boolean): void {
    this.emit('close');
    
    if (!wasClean && this.reconnectAttempts < this.config.maxRetries) {
      this.reconnectAttempts++;
      const delay = this.calculateReconnectDelay();
      console.debug(`Connection failed. Retrying in ${delay}ms...`);
      
      this.reconnectTimeout = window.setTimeout(() => {
        this.connect().catch(error => {
          console.error('Reconnection attempt failed:', error);
        });
      }, delay);
    } else if (!wasClean) {
      console.error('Max reconnection attempts reached');
      this.emit('maxReconnectAttemptsReached');
      this.emit('error', {
        type: MESSAGE_TYPES.ERROR,
        message: 'Maximum reconnection attempts reached',
        code: ERROR_CODES.MAX_RETRIES_EXCEEDED
      });
    }
  }

  private calculateReconnectDelay(): number {
    const baseDelay = this.config.retryDelay;
    const exponentialDelay = baseDelay * Math.pow(2, this.reconnectAttempts - 1);
    const jitter = Math.random() * 1000; // Add up to 1 second of jitter
    return Math.min(exponentialDelay + jitter, 30000); // Cap at 30 seconds
  }

  public send(data: any): void {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      if (this.messageQueue.length < this.config.maxQueueSize) {
        this.messageQueue.push(data);
      } else {
        console.warn('Message queue full, dropping message');
      }
      return;
    }

    const now = Date.now();
    if (now - this.lastMessageTime > this.config.messageTimeWindow) {
      this.messageCount = 0;
      this.lastMessageTime = now;
    }

    if (this.messageCount >= this.config.messageRateLimit) {
      if (this.messageQueue.length < this.config.maxQueueSize) {
        this.messageQueue.push(data);
      }
      return;
    }

    try {
      const message = JSON.stringify(data);
      if (message.length > this.config.maxMessageSize) {
        throw new Error(ERROR_CODES.MESSAGE_TOO_LARGE);
      }
      
      this.ws.send(message);
      this.messageCount++;
      this.lastMessageTime = now;
      this.processQueue();
    } catch (error) {
      console.error('Error sending message:', error);
      const errorMsg: ErrorMessage = {
        type: MESSAGE_TYPES.ERROR,
        message: 'Error sending message',
        code: error instanceof Error && error.message === ERROR_CODES.MESSAGE_TOO_LARGE
          ? ERROR_CODES.MESSAGE_TOO_LARGE
          : ERROR_CODES.INVALID_MESSAGE
      };
      this.emit('error', errorMsg);
    }
  }

  private processQueue(): void {
    while (
      this.messageQueue.length > 0 &&
      this.messageCount < this.config.messageRateLimit
    ) {
      const data = this.messageQueue.shift();
      if (data) {
        this.send(data);
      }
    }
  }

  private processQueuedMessages(): void {
    if (this.messageQueue.length > 0) {
      console.debug(`Processing ${this.messageQueue.length} queued messages`);
      const messages = [...this.messageQueue];
      this.messageQueue = [];
      messages.forEach(message => this.send(message));
    }
  }

  public on<K extends keyof WebSocketEventMap>(
    event: K,
    callback: WebSocketEventCallback<WebSocketEventMap[K]>
  ): void {
    if (!this.eventListeners.has(event)) {
      this.eventListeners.set(event, new Set());
    }
    this.eventListeners.get(event)!.add(callback);
  }

  public off<K extends keyof WebSocketEventMap>(
    event: K,
    callback: WebSocketEventCallback<WebSocketEventMap[K]>
  ): void {
    const listeners = this.eventListeners.get(event);
    if (listeners) {
      listeners.delete(callback);
    }
  }

  private emit<K extends keyof WebSocketEventMap>(
    event: K,
    data?: WebSocketEventMap[K]
  ): void {
    const listeners = this.eventListeners.get(event);
    if (listeners) {
      listeners.forEach(callback => callback(data));
    }
  }

  public cleanup(): void {
    if (this.reconnectTimeout !== null) {
      window.clearTimeout(this.reconnectTimeout);
      this.reconnectTimeout = null;
    }

    if (this.ws) {
      this.ws.onclose = null;
      this.ws.close();
      this.ws = null;
    }

    this.messageQueue = [];
    this.messageCount = 0;
    this.lastMessageTime = 0;
    this.reconnectAttempts = 0;
    this.eventListeners.clear();
  }
}

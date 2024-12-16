/**
 * LogseqXR Application Entry Point
 */

import { platformManager } from './platform/platformManager';
import { settingsManager } from './state/settings';
import { graphDataManager } from './state/graphData';
import { WebSocketService } from './websocket/websocketService';
import { SceneManager } from './rendering/scene';
import { NodeManager } from './rendering/nodes';
import { TextRenderer } from './rendering/textRenderer';
import { XRSessionManager } from './xr/xrSessionManager';
import { XRInteraction } from './xr/xrInteraction';
import { createLogger } from './utils/logger';
import { WS_URL } from './core/constants';
import { BinaryPositionUpdateMessage } from './core/types';
import { ControlPanel } from './ui/ControlPanel';

const logger = createLogger('Application');

class Application {
    private webSocket!: WebSocketService;
    private sceneManager!: SceneManager;
    private nodeManager!: NodeManager;
    private textRenderer!: TextRenderer;
    private xrManager: XRSessionManager | null = null;
    private xrInteraction: XRInteraction | null = null;

    constructor() {
        this.initializeApplication();
    }

    private async initializeApplication(): Promise<void> {
        try {
            // Initialize platform manager
            await platformManager.initialize();

            // Initialize scene first so we can render nodes when data arrives
            this.initializeScene();

            // Load initial graph data from REST endpoint
            await graphDataManager.loadInitialGraphData();
            
            // Load settings from REST endpoint
            await this.loadSettings();

            // Initialize WebSocket for real-time updates
            this.initializeWebSocket();

            // Initialize XR if supported
            await this.initializeXR();

            // Initialize UI components after settings are loaded
            new ControlPanel();

            // Setup UI event listeners
            this.setupUIEventListeners();

            // Hide loading overlay
            this.hideLoadingOverlay();

            logger.log('Application initialized successfully');
        } catch (error) {
            logger.error('Failed to initialize application:', error);
            this.showError('Failed to initialize application');
        }
    }

    private async loadSettings(): Promise<void> {
        try {
            // Initialize settings by loading initial values for each category
            await Promise.all([
                this.loadSettingsCategory('nodes'),
                this.loadSettingsCategory('edges'),
                this.loadSettingsCategory('physics'),
                this.loadSettingsCategory('rendering'),
                this.loadSettingsCategory('bloom'),
                this.loadSettingsCategory('animation'),
                this.loadSettingsCategory('label'),
                this.loadSettingsCategory('ar')
            ]);
        } catch (error) {
            logger.error('Failed to load settings:', error);
            logger.info('Continuing with default settings');
        }
    }

    private async loadSettingsCategory(category: string): Promise<void> {
        const settings = settingsManager.getCurrentSettings();
        const categorySettings = settings[category as keyof typeof settings];
        
        await Promise.all(
            Object.keys(categorySettings).map(async setting => {
                const getterMethod = `get${category.charAt(0).toUpperCase() + category.slice(1)}Setting`;
                await (settingsManager as any)[getterMethod](setting);
            })
        );
    }

    private initializeWebSocket(): void {
        // Create WebSocket service with environment-aware URL
        this.webSocket = new WebSocketService(WS_URL);

        // Setup WebSocket event handler for binary position updates
        this.webSocket.on('binaryPositionUpdate', (data: BinaryPositionUpdateMessage['data']) => {
            if (data && data.nodes) {
                // Convert nodes data to ArrayBuffer for position updates
                const buffer = new ArrayBuffer(data.nodes.length * 24); // 6 floats per node
                const floatArray = new Float32Array(buffer);
                
                data.nodes.forEach((node, index) => {
                    const baseIndex = index * 6;
                    const pos = node.data.position;
                    const vel = node.data.velocity;
                    
                    // Position
                    floatArray[baseIndex] = pos.x;
                    floatArray[baseIndex + 1] = pos.y;
                    floatArray[baseIndex + 2] = pos.z;
                    // Velocity
                    floatArray[baseIndex + 3] = vel.x;
                    floatArray[baseIndex + 4] = vel.y;
                    floatArray[baseIndex + 5] = vel.z;
                });

                // Update graph data and visual representation
                graphDataManager.updatePositions(buffer);
                this.nodeManager.updatePositions(floatArray);
            }
        });

        // Connect to server
        this.webSocket.connect();
    }

    private initializeScene(): void {
        // Get canvas element
        const container = document.getElementById('canvas-container');
        if (!container) {
            throw new Error('Canvas container not found');
        }

        // Create canvas
        const canvas = document.createElement('canvas');
        container.appendChild(canvas);

        // Initialize scene manager
        this.sceneManager = SceneManager.getInstance(canvas);

        // Initialize node manager
        this.nodeManager = NodeManager.getInstance(this.sceneManager);

        // Initialize text renderer
        this.textRenderer = TextRenderer.getInstance(
            this.sceneManager.getScene(),
            this.sceneManager.getCamera()
        );

        // Start rendering
        this.sceneManager.start();
    }

    private async initializeXR(): Promise<void> {
        if (platformManager.getCapabilities().xrSupported) {
            // Initialize XR manager
            this.xrManager = XRSessionManager.getInstance(this.sceneManager);

            // Initialize XR interaction
            if (this.xrManager && this.nodeManager) {
                this.xrInteraction = XRInteraction.getInstance(this.xrManager, this.nodeManager);
            }

            // Setup XR button
            const xrButton = document.getElementById('xr-button');
            if (xrButton) {
                xrButton.style.display = 'block';
                xrButton.addEventListener('click', () => this.toggleXRSession());
            }
        }
    }

    private setupUIEventListeners(): void {
        // Settings panel save button
        const saveButton = document.getElementById('save-settings');
        if (saveButton) {
            saveButton.addEventListener('click', () => this.saveSettings());
        }

        // Settings inputs
        this.setupSettingsInputListeners();
    }

    private setupSettingsInputListeners(): void {
        // Node appearance settings
        this.setupSettingInput('nodeSize', 'number');
        this.setupSettingInput('nodeColor', 'color');
        this.setupSettingInput('nodeOpacity', 'number');

        // Edge appearance settings
        this.setupSettingInput('edgeWidth', 'number');
        this.setupSettingInput('edgeColor', 'color');
        this.setupSettingInput('edgeOpacity', 'number');

        // Visual effects settings
        this.setupSettingInput('enableBloom', 'checkbox');
        this.setupSettingInput('bloomIntensity', 'number');
    }

    private setupSettingInput(id: string, type: 'number' | 'color' | 'checkbox'): void {
        const input = document.getElementById(id) as HTMLInputElement;
        if (input) {
            input.addEventListener('change', () => {
                const value = type === 'checkbox' ? input.checked :
                            type === 'number' ? parseFloat(input.value) :
                            input.value;
                // Update settings and save to server via REST endpoint
                settingsManager.updateSettings({ [id]: value });
            });
        }
    }

    private async saveSettings(): Promise<void> {
        try {
            await settingsManager.saveSettings();
            logger.log('Settings saved successfully');
        } catch (error) {
            logger.error('Failed to save settings:', error);
            this.showError('Failed to save settings');
        }
    }

    private async toggleXRSession(): Promise<void> {
        if (!this.xrManager) return;

        try {
            if (this.xrManager.isXRPresenting()) {
                await this.xrManager.endXRSession();
            } else {
                await this.xrManager.initXRSession();
            }
        } catch (error) {
            logger.error('Failed to toggle XR session:', error);
            this.showError('Failed to start XR session');
        }
    }

    private hideLoadingOverlay(): void {
        const overlay = document.querySelector('.loading-overlay');
        if (overlay) {
            overlay.remove();
        }
    }

    private showError(message: string): void {
        logger.error(message);
        const errorDiv = document.createElement('div');
        errorDiv.style.cssText = `
            position: fixed;
            top: 20px;
            right: 20px;
            background: rgba(255, 0, 0, 0.8);
            color: white;
            padding: 15px;
            border-radius: 5px;
            z-index: 1000;
        `;
        errorDiv.textContent = message;
        document.body.appendChild(errorDiv);
        setTimeout(() => errorDiv.remove(), 5000);
    }

    dispose(): void {
        // Dispose of managers in reverse order of initialization
        settingsManager.dispose();
        this.xrInteraction?.dispose();
        this.xrManager?.dispose();
        this.textRenderer.dispose();
        this.nodeManager.dispose();
        this.sceneManager.dispose();

        // Stop rendering
        this.sceneManager.stop();

        // Close WebSocket connection if it exists
        if (this.webSocket) {
            this.webSocket.disconnect();
        }
    }
}

// Create application instance
const app = new Application();

// Handle window unload
window.addEventListener('unload', () => {
    app.dispose();
});

// Log application start
console.info('LogseqXR application starting...');

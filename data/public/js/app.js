// data/public/js/app.js

import { createApp } from 'vue';
import ControlPanel from './components/ControlPanel.vue';
import ChatManager from './components/chatManager.vue';
import { WebXRVisualization } from './components/visualization/core.js';
import WebsocketService from './services/websocketService.js';
import { GraphDataManager } from './services/graphDataManager.js';
import { isGPUAvailable, initGPU } from './gpuUtils.js';
import { enableSpacemouse } from './services/spacemouse.js';

export class App {
    constructor() {
        console.log('App constructor called');
        this.websocketService = null;
        this.graphDataManager = null;
        this.visualization = null;
        this.gpuAvailable = false;
        this.gpuUtils = null;
        this.initializeApp();
    }

    initializeApp() {
        console.log('Initializing Application - Step 1: Services');

        // Initialize Services
        try {
            this.websocketService = new WebsocketService();
            console.log('WebsocketService initialized successfully');
        } catch (error) {
            console.error('Failed to initialize WebsocketService:', error);
        }

        if (this.websocketService) {
            this.graphDataManager = new GraphDataManager(this.websocketService);
            console.log('GraphDataManager initialized successfully');
        } else {
            console.error('Cannot initialize GraphDataManager: WebsocketService is not available');
        }
        
        console.log('Initializing Application - Step 2: Visualization');
        try {
            // Add container check
            const container = document.getElementById('scene-container');
            if (!container) {
                console.error('Scene container not found, creating it');
                const newContainer = document.createElement('div');
                newContainer.id = 'scene-container';
                document.body.appendChild(newContainer);
            }

            this.visualization = new WebXRVisualization(this.graphDataManager);
            console.log('WebXRVisualization initialized successfully');
        } catch (error) {
            console.error('Failed to initialize WebXRVisualization:', error);
            console.error('Error stack:', error.stack);
        }

        console.log('Initializing Application - Step 3: GPU');
        // Initialize GPU if available
        this.gpuAvailable = isGPUAvailable();
        if (this.gpuAvailable) {
            this.gpuUtils = initGPU();
            console.log('GPU acceleration initialized');
        } else {
            console.warn('GPU acceleration not available, using CPU fallback');
        }

        console.log('Initializing Application - Step 4: Vue App');
        // Initialize Vue App with ChatManager and ControlPanel
        this.initVueApp();

        console.log('Initializing Application - Step 5: Event Listeners');
        // Setup Event Listeners
        this.setupEventListeners();

        console.log('Initializing Application - Step 6: Three.js');
        // Initialize the visualization
        if (this.visualization) {
            this.visualization.initThreeJS();
        } else {
            console.error('Visualization not initialized, cannot call initThreeJS');
        }
    }

    initVueApp() {
        console.log('Initializing Vue App - Start');
        const websocketService = this.websocketService;
        const visualization = this.visualization;
        const graphDataManager = this.graphDataManager;

        // Check if Vue is available
        if (typeof createApp !== 'function') {
            console.error('Vue createApp not found!');
            return;
        }

        const app = createApp({
            components: {
                ControlPanel,
                ChatManager
            },
            setup() {
                console.log('Vue app setup function called');
                const handleControlChange = (data) => {
                    console.log('Control changed:', data.name, data.value);
                    if (visualization) {
                        console.log('Updating visualization:', data);
                        
                        // Handle force-directed graph parameters
                        if (data.name === 'force_directed_iterations' || 
                            data.name === 'force_directed_spring' ||
                            data.name === 'force_directed_repulsion' || 
                            data.name === 'force_directed_attraction' ||
                            data.name === 'force_directed_damping') {
                            updateForceDirectedParams(data.name, data.value);
                        } else {
                            // Pass name and value separately to updateVisualFeatures
                            visualization.updateVisualFeatures(data.name, data.value);
                        }
                    } else {
                        console.error('Cannot update visualization: not initialized');
                    }
                };

                const updateForceDirectedParams = (name, value) => {
                    if (graphDataManager) {
                        // Update the force-directed parameters in the graph data manager
                        graphDataManager.updateForceDirectedParams(name, value);
                        
                        // Trigger a recalculation of the graph layout
                        graphDataManager.recalculateLayout();
                        
                        // Update the visualization with the new layout
                        visualization.updateVisualization();
                    } else {
                        console.error('Cannot update force-directed parameters: GraphDataManager not initialized');
                    }
                };

                const toggleFullscreen = () => {
                    if (document.fullscreenElement) {
                        document.exitFullscreen();
                    } else {
                        document.documentElement.requestFullscreen();
                    }
                };

                return {
                    websocketService,
                    handleControlChange,
                    toggleFullscreen,
                    enableSpacemouse
                };
            },
            template: `
                <div id="app-container">
                    <chat-manager :websocketService="websocketService"></chat-manager>
                    <control-panel 
                        :websocketService="websocketService"
                        @control-change="handleControlChange"
                        @toggle-fullscreen="toggleFullscreen"
                        @enable-spacemouse="enableSpacemouse"
                        style="z-index: 1000;"
                    ></control-panel>
                </div>
            `
        });

        // Mount to a specific element
        const mountElement = document.getElementById('app');
        if (!mountElement) {
            console.error('Could not find #app element for mounting Vue application');
            return;
        }

        try {
            app.mount('#app');
            console.log('Vue App mounted successfully');
        } catch (error) {
            console.error('Failed to mount Vue app:', error);
        }
    }

    setupEventListeners() {
        console.log('Setting up event listeners');

        if (this.websocketService) {
            // WebSocket Event Listeners
            this.websocketService.on('open', () => {
                console.log('WebSocket connection established');
                this.updateConnectionStatus(true);
                if (this.graphDataManager) {
                    this.graphDataManager.requestInitialData();
                } else {
                    console.error('GraphDataManager not initialized, cannot request initial data');
                }
            });

            this.websocketService.on('message', (data) => {
                console.log('WebSocket message received:', data);
                this.handleWebSocketMessage(data);
            });

            this.websocketService.on('error', (error) => {
                console.error('WebSocket error:', error);
                this.updateConnectionStatus(false);
            });

            this.websocketService.on('close', () => {
                console.log('WebSocket connection closed');
                this.updateConnectionStatus(false);
            });
        } else {
            console.error('WebsocketService not initialized, cannot set up WebSocket listeners');
        }

        // Custom Event Listener for Graph Data Updates
        window.addEventListener('graphDataUpdated', (event) => {
            console.log('Graph data updated event received', event.detail);
            if (this.visualization) {
                this.visualization.updateVisualization();
            } else {
                console.error('Cannot update visualization: not initialized');
            }
        });

        // Spacemouse Move Event Listener
        window.addEventListener('spacemouse-move', (event) => {
            const { x, y, z } = event.detail;
            if (this.visualization) {
                this.visualization.handleSpacemouseInput(x, y, z);
            } else {
                console.error('Cannot handle Spacemouse input: Visualization not initialized');
            }
        });

        // Initialize audio on first user interaction
        const initAudio = () => {
            if (this.websocketService) {
                this.websocketService.initAudio();
            }
        };

        document.addEventListener('click', initAudio, { once: true });
        document.addEventListener('touchstart', initAudio, { once: true });
    }

    handleWebSocketMessage(data) {
        console.log('Handling WebSocket message:', data);
        switch (data.type) {
            case 'getInitialData':
                console.log('Received initial data:', data);
                if (data.graph_data && this.graphDataManager) {
                    this.graphDataManager.updateGraphData(data.graph_data);
                    if (this.visualization) {
                        this.visualization.updateVisualization();
                    }
                }
                if (data.settings) {
                    console.log('Received settings:', data.settings);
                    if (this.visualization) {
                        this.visualization.updateSettings(data.settings);
                    }
                    window.dispatchEvent(new CustomEvent('serverSettings', {
                        detail: data.settings
                    }));
                } else {
                    console.warn('No settings received in initial data');
                }
                break;
            case 'graphUpdate':
                console.log('Received graph update:', data.graphData);
                if (this.graphDataManager) {
                    this.graphDataManager.updateGraphData(data.graphData);
                    if (this.visualization) {
                        this.visualization.updateVisualization();
                    }
                }
                break;
            case 'ttsMethodSet':
                console.log('TTS method set:', data.method);
                break;
            case 'error':
                console.error('Server error:', data.message);
                if (this.visualization) {
                    this.visualization.showError(data.message);
                }
                break;
            default:
                console.warn(`Unhandled message type: ${data.type}`, data);
                break;
        }
    }

    updateConnectionStatus(isConnected) {
        const statusElement = document.getElementById('connection-status');
        if (statusElement) {
            statusElement.textContent = isConnected ? 'Connected' : 'Disconnected';
            statusElement.className = isConnected ? 'connected' : 'disconnected';
        } else {
            console.warn('Connection status element not found');
        }
    }

    start() {
        console.log('Starting the application');
        if (this.visualization) {
            console.log('Starting visualization animation');
            this.visualization.animate();
        } else {
            console.error('Cannot start animation: Visualization not initialized');
        }
    }
}

// Initialize the App once the DOM content is fully loaded
document.addEventListener('DOMContentLoaded', () => {
    console.log('DOM fully loaded, creating App instance');
    const app = new App();
    app.start();
});

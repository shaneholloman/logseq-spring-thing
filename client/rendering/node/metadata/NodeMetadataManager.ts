import {
    Object3D,
    Camera,
    Scene,
    Vector3,
    Sprite,
    SpriteMaterial,
    Texture
} from 'three';
import { NodeMetadata } from '../../../types/metadata';
import { createLogger, createDataMetadata } from '../../../core/logger';
import { Settings } from '../../../types/settings';
import { debugState } from '../../../core/debugState';

const logger = createLogger('NodeMetadataManager');

interface MetadataLabel {
    sprite: Sprite;
    container: Object3D;
    metadata: NodeMetadata;
    lastUpdateDistance: number;
    lastVisible: boolean;
}

export class NodeMetadataManager {
    private static instance: NodeMetadataManager;
    private labels: Map<string, MetadataLabel> = new Map();
    // Add a map to store relationships between node IDs and metadata IDs (filenames)
    private nodeIdToMetadataId: Map<string, string> = new Map();
    private metadataIdToNodeId: Map<string, string> = new Map();
    private VISIBILITY_THRESHOLD = 100;  // Increased maximum distance for label visibility
    private readonly UPDATE_INTERVAL = 2;        // More frequent updates
    private settings: Settings;
    private readonly LABEL_SCALE = 0.5;         // Base scale for labels
    private readonly DEFAULT_FILE_SIZE = 1000; // Default fileSize for fallback
    private frameCount = 0;
    private isInitialMappingComplete = false;

    private worldPosition = new Vector3();
    private labelCanvas: HTMLCanvasElement;
    private labelContext: CanvasRenderingContext2D;
    private scene: Scene;

    private constructor(scene: Scene, settings?: Settings) {
        this.settings = settings || {} as Settings;
        // Create canvas for label textures
        this.labelCanvas = document.createElement('canvas');
        this.labelCanvas.width = 256;
        this.labelCanvas.height = 128;
        
        const context = this.labelCanvas.getContext('2d');
        if (!context) {
            throw new Error('Failed to get 2D context for label canvas');
        }
        this.labelContext = context;
        
        // Set up default text style
        this.labelContext.textAlign = 'center';
        this.labelContext.textBaseline = 'middle';
        this.labelContext.font = 'bold 24px Arial';
        
        this.scene = scene;
    }

    public static getInstance(scene?: Scene, settings?: Settings): NodeMetadataManager {
        if (!NodeMetadataManager.instance) {
            NodeMetadataManager.instance = new NodeMetadataManager(
                scene || new Scene(), settings);
        }
        return NodeMetadataManager.instance;
    }

    /**
     * Prioritize mapping node IDs to metadata IDs before any other operation
     * This is critical for ensuring labels are displayed correctly from the start
     * @param nodes Array of nodes to establish initial mappings
     */
    public initializeMappings(nodes: Array<{id: string, metadataId?: string, label?: string}>): void {
        if (this.isInitialMappingComplete) return;
        
        logger.info(`Initializing metadata mappings for ${nodes.length} nodes`, 
            createDataMetadata({ nodeCount: nodes.length }));
        
        nodes.forEach(node => {
            // Always prefer metadataId (filename) over label for mapping
            const metadataId = node.metadataId || node.label || node.id;
            if (metadataId && metadataId !== node.id) {
                this.mapNodeIdToMetadataId(node.id, metadataId);
                logger.info(`Initial mapping: Node ID ${node.id} -> metadata ID "${metadataId}"`);
            }
        });
        
        this.isInitialMappingComplete = true;
        logger.info(`Completed initial metadata mappings for ${this.nodeIdToMetadataId.size} nodes`);
    }

    /**
     * Handle settings updates
     * @param settings The new settings object
     */
    public handleSettingsUpdate(settings: Settings): void {
        // Update our local settings reference
        this.settings = settings;

        // Check for visualization settings - may need to adapt this path 
        // based on your actual settings structure
        const visibilityThreshold = settings.visualization?.labels?.visibilityThreshold;
        
        // Only update if we have a valid number
        if (typeof visibilityThreshold === 'number') {
            // Using the settings directly in the code to ensure TS sees it's being used
            this.VISIBILITY_THRESHOLD = visibilityThreshold;
            this.updateVisibilityThreshold(visibilityThreshold);
        }
    }

    private createLabelTexture(metadata: NodeMetadata): Texture {
        // Clear canvas
        this.labelContext.clearRect(0, 0, this.labelCanvas.width, this.labelCanvas.height);

        // CRITICAL: First check if we have a mapping for this node ID
        let displayName = this.nodeIdToMetadataId.get(metadata.id) || metadata.name || metadata.id || 'Unknown';
        
        // Using settings property to control debug logging
        const enableDebugLogging = debugState.isNodeDebugEnabled() || 
            (this.settings?.system?.debug?.enabled === true);
        if (enableDebugLogging) {
            logger.debug(`Creating label for node ${metadata.id} with name: ${displayName}`,
                createDataMetadata({ 
                    originalName: metadata.name,
                    mappedName: this.nodeIdToMetadataId.get(metadata.id),
                    fileSize: metadata.fileSize 
                }));
        }
        
        logger.info(`Label texture for node ${metadata.id}: Using name "${displayName}"`);

        // Draw a slightly larger background to accommodate multiple lines
        this.labelContext.fillStyle = 'rgba(0, 0, 0, 0.5)';
        this.labelContext.fillRect(0, 0, this.labelCanvas.width, this.labelCanvas.height);

        // Draw main label (filename)
        this.labelContext.fillStyle = 'white';
        this.labelContext.font = 'bold 20px Arial';
        this.labelContext.fillText(displayName, this.labelCanvas.width / 2, 30);
        
        // Draw subtext lines
        this.labelContext.font = '14px Arial';
        this.labelContext.fillStyle = '#dddddd';
        
        // Add file size if available
        if (metadata.fileSize) {
            // Use actual fileSize or a reasonable default
            const fileSize = metadata.fileSize || this.DEFAULT_FILE_SIZE;
            const fileSizeText = this.formatFileSize(fileSize);
            this.labelContext.fillText(`Size: ${fileSizeText}`, this.labelCanvas.width / 2, 55);
        }
        
        // Add hyperlink count if available
        if (metadata.hyperlinkCount !== undefined && metadata.hyperlinkCount > 0) {
            this.labelContext.fillText(`Links: ${metadata.hyperlinkCount}`, this.labelCanvas.width / 2, 75); 
        }

        // Create texture
        const texture = new Texture(this.labelCanvas);
        texture.needsUpdate = true;
        return texture;
    }
    
    /**
     * Format file size into human-readable format
     */
    private formatFileSize(bytes: number): string {
        if (bytes < 1024) return bytes + ' B';
        if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
        if (bytes < 1024 * 1024 * 1024) return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
        return (bytes / (1024 * 1024 * 1024)).toFixed(1) + ' GB';
    }

    public async createMetadataLabel(metadata: NodeMetadata): Promise<Object3D> {
        // The problem: We were using a shared canvas instance for all node labels
        // Create a unique texture for each node with its own canvas instance
        logger.info(`Creating metadata label for node ID: ${metadata.id}, metadata name: ${metadata.name}`, 
            createDataMetadata({ nodeId: metadata.id, name: metadata.name }));
        
        // Create a dedicated canvas for this label
        const canvas = document.createElement('canvas');
        canvas.width = 256;
        canvas.height = 128;
        
        const context = canvas.getContext('2d');
        if (!context) {
            throw new Error('Failed to get 2D context for dedicated canvas');
        }
        
        // Configure the context the same way as our shared one
        context.textAlign = 'center';
        context.textBaseline = 'middle';
        context.font = 'bold 24px Arial';
        
        // Use settings for font size if available
        const fontSize = this.settings?.visualization?.labels?.desktopFontSize || 24;
        context.font = `bold ${fontSize}px Arial`;
        
        // CRITICAL FIX: Simplified name resolution logic - consistent with createLabelTexture
        // First check node-to-metadata mapping, then fall back to metadata name
        let displayName = this.nodeIdToMetadataId.get(metadata.id) || 
                         (metadata.name && metadata.name !== 'undefined' ? metadata.name : metadata.id) || 'Unknown';

        // Log the name resolution process for debugging
        if (debugState.isNodeDebugEnabled()) {
            logger.debug(`Node ${metadata.id} display name resolution:`, 
                createDataMetadata({ mappedName: this.nodeIdToMetadataId.get(metadata.id),
                                    metadataName: metadata.name,
                                    finalDisplayName: displayName,
                                    fileSize: metadata.fileSize, 
                                       hyperlinkCount: metadata.hyperlinkCount }));
        }
        
        // Draw background
        context.fillStyle = 'rgba(0, 0, 0, 0.5)';
        context.fillRect(0, 0, canvas.width, canvas.height);
        
        // Draw main label (filename)
        context.fillStyle = 'white';
        context.font = 'bold 20px Arial';
        // Truncate very long names
        let displayText = displayName;
        if (displayText.length > 30) {
            displayText = displayText.substring(0, 27) + '...';
        }
        context.fillText(displayText, canvas.width / 2, 30);
        
        // Draw subtext lines
        context.font = '14px Arial';
        context.fillStyle = '#dddddd';
        
        // Add file size if available
        if (metadata.fileSize !== undefined && metadata.fileSize > 0) {
            const fileSizeText = this.formatFileSize(metadata.fileSize);
            context.fillText(`Size: ${fileSizeText}`, canvas.width / 2, 55);
        } else {
            // Debug message for missing file size
            if (debugState.isNodeDebugEnabled()) {
                logger.debug(`Node ${metadata.id} (${displayName}) missing file size`, 
                    createDataMetadata({ 
                        metadataFileSize: metadata.fileSize || 'undefined'
                    }));
            }
        }
        
        // Add hyperlink count if available
        if (metadata.hyperlinkCount !== undefined && metadata.hyperlinkCount > 0) {
            context.fillText(`Links: ${metadata.hyperlinkCount}`, canvas.width / 2, 75);
        } else {
            // Debug message for missing hyperlink count only if debug is enabled
            if (debugState.isNodeDebugEnabled()) {
                logger.debug(`Node ${metadata.id} (${displayName}) missing hyperlink count`, 
                    createDataMetadata({ 
                        metadataHyperlinkCount: metadata.hyperlinkCount || 'undefined'
                    }));
            }
        }

        
        // Create a unique texture from this canvas instance
        const texture = new Texture(canvas);
        texture.needsUpdate = true;
        
        const material = new SpriteMaterial({
            map: texture,
            color: 0xffffff,
            transparent: true,
            opacity: 0.9,  // Slightly increased for better visibility
            // Ensure the renderer knows this material is unique
            depthWrite: false  // Changed to false to prevent occlusion by other objects
        });

        const sprite = new Sprite(material);
        sprite.scale.set(this.LABEL_SCALE * 2, this.LABEL_SCALE, 1);
        sprite.renderOrder = 1000; // Significantly increase render order to ensure labels are always on top

        // Add the sprite to an Object3D container for better control
        const container = new Object3D();
        container.add(sprite);
        container.renderOrder = 1000; // Ensure the entire container has high render priority
        container.position.copy(sprite.position); // Initialize container position
        
        // Enable both layers for desktop mode
        sprite.layers.enable(0);
        sprite.layers.enable(1);

        const label: MetadataLabel = {
            sprite,
            container,
            metadata,
            lastUpdateDistance: Infinity,
            lastVisible: false
        };

        // Add to scene
        this.scene.add(container);

        // If this node has a numeric ID, map it to the display name
        if (displayName && displayName !== metadata.id) {
            // Add or update the mapping
            const existingMapping = this.nodeIdToMetadataId.get(metadata.id);
            if (!existingMapping || existingMapping !== displayName) {
                this.mapNodeIdToMetadataId(metadata.id, displayName);
                logger.info(`Updated mapping: node ID ${metadata.id} -> "${displayName}"`);
            }
        }

        this.labels.set(metadata.id, label);
        return sprite;
    }

    public update(camera: Camera): void {
        this.frameCount++;
        if (this.frameCount % this.UPDATE_INTERVAL !== 0) return;

        const cameraPosition = camera.position;

        this.labels.forEach((label) => {
            const { sprite, container, metadata } = label;
            
            // Get actual world position from metadata
            this.worldPosition.set(
                metadata.position.x || 0,
                metadata.position.y || 0,
                metadata.position.z || 0
            );
            
            // Update sprite position
            container.position.copy(this.worldPosition);
            
            const distance = this.worldPosition.distanceTo(cameraPosition);

            // Update visibility based on distance
            const visible = distance < this.VISIBILITY_THRESHOLD;
            sprite.visible = visible;

            if (label.lastVisible !== visible) {
                label.lastVisible = visible;
            }

            if (visible) {
                // Scale based on distance
                const scale = Math.max(0.5, 1 - (distance / this.VISIBILITY_THRESHOLD));
                sprite.scale.set(
                    this.LABEL_SCALE * scale * 2,
                    this.LABEL_SCALE * scale,
                    1
                );

                // Make sprite face camera
                container.lookAt(cameraPosition);
            }

            // Update last known distance
            label.lastUpdateDistance = distance;
        });
    }

    public updateMetadata(id: string, metadata: NodeMetadata): void {
        const label = this.labels.get(id);
        if (!label) {
            this.createMetadataLabel(metadata);
            return;
        }

        // Update metadata
        const oldMetadata = { ...label.metadata };
        label.metadata = metadata;
        
        // Check if we need to update the node-to-metadata mapping
        if (metadata.name && metadata.name !== oldMetadata.name) {
            this.mapNodeIdToMetadataId(metadata.id, metadata.name);
            if (debugState.isNodeDebugEnabled()) {
                logger.debug(`Updated metadata mapping: ${metadata.id} -> ${metadata.name}`);
            } else {
                // Log at info level for important mappings to help diagnose issues
                if (/^\d+$/.test(metadata.id)) {
                    logger.info(`Updated numeric ID mapping: ${metadata.id} -> ${metadata.name}`);
                }
            }
        }
        
        // Update texture
        const texture = this.createLabelTexture(metadata); 
        
        // Dispose of old texture to avoid memory leaks
        if ((label.sprite.material as SpriteMaterial).map) {
            (label.sprite.material as SpriteMaterial).map?.dispose();
        }
        // Update material with new texture
        (label.sprite.material as SpriteMaterial).map = texture;
        (label.sprite.material as SpriteMaterial).needsUpdate = true;
    }
    
    /**
     * Map a node ID to a metadata ID (filename) for proper labeling
     * This is crucial for connecting numeric IDs with human-readable names
     */
    public mapNodeIdToMetadataId(nodeId: string, metadataId: string): void {
        // Don't map empty metadata IDs
        if (!metadataId || metadataId === 'undefined' || metadataId === 'Unknown') {
            if (debugState.isNodeDebugEnabled()) {
                logger.debug(`Skipping invalid metadata ID mapping for node ${nodeId}: "${metadataId}"`);
            }
            return;
        }
        
        // Log previous mapping if it exists and is different
        const prevMapping = this.nodeIdToMetadataId.get(nodeId);
        if (prevMapping && prevMapping !== metadataId) {
            logger.info(`Updating node ID ${nodeId} mapping from "${prevMapping}" to "${metadataId}"`, 
                createDataMetadata({ nodeId, previousMapping: prevMapping, newMapping: metadataId }));
        }
        
        this.nodeIdToMetadataId.set(nodeId, metadataId);
        // Maintain reverse mapping for bidirectional lookup
        if (!this.metadataIdToNodeId.has(metadataId)) {
            this.metadataIdToNodeId.set(metadataId, nodeId);
        }
        
        // Only log new mappings or if debug is enabled
        if (!prevMapping || debugState.isNodeDebugEnabled()) {
            logger.info(`Mapped node ID ${nodeId} to metadata ID "${metadataId}"`);
        }
    }

    public updatePosition(id: string, position: Vector3): void {
        const label = this.labels.get(id);
        if (!label) {
            // Check if this is a numeric ID with a mapped metadata ID
            if (/^\d+$/.test(id) && this.nodeIdToMetadataId.has(id)) {
                const metadataId = this.nodeIdToMetadataId.get(id);
                // Try to find the label using the metadata ID
                const metadataLabel = this.labels.get(metadataId!);
                if (metadataLabel) {
                    // Update the metadata label position
                    metadataLabel.metadata.position = { 
                        x: position.x, 
                        y: position.y, 
                        z: position.z 
                    };
                    metadataLabel.container.position.copy(position);
                    return;
                }
            }
            
            // Only log missing labels in debug mode to avoid spamming the console
            if (debugState.isNodeDebugEnabled()) {
                logger.debug(`No label found for node ${id}`);
            }
            return;
        }

        // Update metadata position
        label.metadata.position = { x: position.x, y: position.y, z: position.z };
        // Update sprite position
        label.container.position.copy(position);
    }

    public updateVisibilityThreshold(threshold: number): void {
        if (threshold > 0) {
            this.VISIBILITY_THRESHOLD = threshold;
            if (debugState.isNodeDebugEnabled()) {
                logger.debug(`Updated visibility threshold to ${threshold}`);
            }
        }
    }

    public setXRMode(enabled: boolean): void {
        this.labels.forEach((label) => {
            const sprite = label.sprite;
            if (enabled) {
                // XR mode - only layer 1
                sprite.layers.disable(0);
                sprite.layers.enable(1);
            } else {
                // Desktop mode - both layers
                sprite.layers.enable(0);
                sprite.layers.enable(1);
            }
        });
    }

    /**
     * Get the metadata ID (filename) for a given node ID
     */
    public getMetadataId(nodeId: string): string | undefined {
        return this.nodeIdToMetadataId.get(nodeId);
    }

    /**
     * Get the node ID for a given metadata ID (filename)
     */
    public getNodeId(metadataId: string): string | undefined {
        return this.metadataIdToNodeId.get(metadataId);
    }

    /**
     * Get the label for a node - uses the mapped metadata name if available
     */
    public getLabel(nodeId: string): string {
        return this.nodeIdToMetadataId.get(nodeId) || nodeId;
    }

    public removeLabel(id: string): void {
        const label = this.labels.get(id);
        if (!label) return;

        // Clean up resources
        (label.sprite.material as SpriteMaterial).map?.dispose();
        label.sprite.material.dispose();

        // Remove from scene
        this.scene.remove(label.container);
        
        // Remove from tracking
        this.labels.delete(id);
    }

    public dispose(): void {
        // Clean up all labels
        this.labels.forEach((label) => {
            (label.sprite.material as SpriteMaterial).map?.dispose();
            label.sprite.material.dispose();
            
            // Remove from scene
            this.scene.remove(label.container);
        });
        this.labels.clear();
        
        // Clear mappings
        this.nodeIdToMetadataId.clear();
        this.metadataIdToNodeId.clear();

        // Reset singleton
        NodeMetadataManager.instance = null!;
        logger.info('Disposed NodeMetadataManager');
    }
}
import * as THREE from 'three';
import { visualizationSettings } from '../../services/visualizationSettings.js';

// Constants
export const BLOOM_LAYER = 1;
export const NORMAL_LAYER = 0;

export class NodeManager {
    constructor(scene, camera, settings = {}) {
        this.scene = scene;
        this.camera = camera;
        this.nodeMeshes = new Map();
        this.nodeLabels = new Map();
        this.edgeMeshes = new Map();
        this.nodeData = new Map();
        
        this.raycaster = new THREE.Raycaster();
        this.mouse = new THREE.Vector2();
        
        // Get settings from visualization settings service
        const nodeSettings = visualizationSettings.getNodeSettings();
        
        // Physical dimensions in meters
        this.minNodeSize = settings.minNodeSize || nodeSettings.minNodeSize; // 0.1m = 10cm
        this.maxNodeSize = settings.maxNodeSize || nodeSettings.maxNodeSize; // 0.3m = 30cm
        
        // Visual settings
        this.labelFontSize = settings.labelFontSize || nodeSettings.labelFontSize;
        this.nodeColor = new THREE.Color(settings.nodeColor || nodeSettings.color);
        this.materialSettings = nodeSettings.material;
        this.ageColors = {
            NEW: new THREE.Color(nodeSettings.colorNew),
            RECENT: new THREE.Color(nodeSettings.colorRecent),
            MEDIUM: new THREE.Color(nodeSettings.colorMedium),
            OLD: new THREE.Color(nodeSettings.colorOld)
        };
        this.maxAge = nodeSettings.ageMaxDays;

        // Edge settings
        const edgeSettings = visualizationSettings.getEdgeSettings();
        this.edgeColor = new THREE.Color(settings.edgeColor || edgeSettings.color);
        this.edgeOpacity = settings.edgeOpacity || edgeSettings.opacity;

        this.handleClick = this.handleClick.bind(this);
        this.xrEnabled = false;
        this.xrLabelManager = null;
    }

    getNodeSize(metadata) {
        // Calculate node size in meters based on metadata
        if (metadata.node_size) {
            const size = parseFloat(metadata.node_size);
            // Normalize size between minNodeSize (0.1m) and maxNodeSize (0.3m)
            return this.minNodeSize + (size * (this.maxNodeSize - this.minNodeSize));
        }
        return this.minNodeSize; // Default to minimum size (10cm)
    }

    calculateNodeColor(metadata) {
        // Use github_last_modified if available, otherwise fall back to last_modified
        const lastModified = metadata.github_last_modified || metadata.last_modified || new Date().toISOString();
        const now = Date.now();
        const age = now - new Date(lastModified).getTime();
        const dayInMs = 24 * 60 * 60 * 1000;
        
        if (age < 3 * dayInMs) return this.ageColors.NEW;        // Less than 3 days old
        if (age < 7 * dayInMs) return this.ageColors.RECENT;     // Less than 7 days old
        if (age < 30 * dayInMs) return this.ageColors.MEDIUM;    // Less than 30 days old
        return this.ageColors.OLD;                               // 30 days or older
    }

    createNodeGeometry(size, hyperlinkCount) {
        // Create a sphere with radius in meters
        // Scale segments based on hyperlink count for performance vs. quality
        const minSegments = visualizationSettings.getNodeSettings().geometryMinSegments;
        const maxSegments = visualizationSettings.getNodeSettings().geometryMaxSegments;
        const segmentPerLink = visualizationSettings.getNodeSettings().geometrySegmentPerHyperlink;
        
        const segments = Math.min(
            maxSegments,
            Math.max(minSegments, Math.floor(hyperlinkCount * segmentPerLink) + minSegments)
        );
        
        return new THREE.SphereGeometry(size, segments, segments);
    }

    createNodeMaterial(color, metadata) {
        // Use github_last_modified if available, otherwise fall back to last_modified
        const lastModified = metadata.github_last_modified || metadata.last_modified || new Date().toISOString();
        const now = Date.now();
        const ageInDays = (now - new Date(lastModified).getTime()) / (24 * 60 * 60 * 1000);
        
        // Normalize age to 0-1 range and invert (newer = brighter)
        const normalizedAge = Math.min(ageInDays / this.maxAge, 1);
        const emissiveIntensity = this.materialSettings.emissiveMaxIntensity - 
            (normalizedAge * (this.materialSettings.emissiveMaxIntensity - this.materialSettings.emissiveMinIntensity));

        return new THREE.MeshPhysicalMaterial({
            color: color,
            emissive: color,
            emissiveIntensity: emissiveIntensity,
            metalness: this.materialSettings.metalness,
            roughness: this.materialSettings.roughness,
            transparent: true,
            opacity: this.materialSettings.opacity,
            envMapIntensity: 1.0,
            clearcoat: this.materialSettings.clearcoat,
            clearcoatRoughness: this.materialSettings.clearcoatRoughness
        });
    }

    createNodeLabel(text, metadata) {
        const canvas = document.createElement('canvas');
        const context = canvas.getContext('2d');
        context.font = `${this.labelFontSize}px Arial`;
        
        // Get metadata values
        const fileSize = parseInt(metadata.file_size) || 1;
        const lastModified = metadata.github_last_modified || metadata.last_modified || new Date().toISOString();
        const hyperlinkCount = parseInt(metadata.hyperlink_count) || 0;
        const githubInfo = metadata.github_info || {};
        
        // Measure and create text
        const nameMetrics = context.measureText(text);
        let infoText = `${this.formatFileSize(fileSize)} | ${this.formatAge(lastModified)} | ${hyperlinkCount} links`;
        if (githubInfo.author) {
            infoText += ` | ${githubInfo.author}`;
        }
        if (githubInfo.commit_message) {
            const shortMessage = githubInfo.commit_message.split('\n')[0].slice(0, 30);
            infoText += ` | ${shortMessage}${githubInfo.commit_message.length > 30 ? '...' : ''}`;
        }
        
        const infoMetrics = context.measureText(infoText);
        const textWidth = Math.max(nameMetrics.width, infoMetrics.width);
        
        // Set canvas size
        canvas.width = textWidth + 20;
        canvas.height = this.labelFontSize * 2 + 30;

        // Draw background and text
        context.fillStyle = visualizationSettings.getLabelSettings().backgroundColor;
        context.fillRect(0, 0, canvas.width, canvas.height);

        context.font = `${this.labelFontSize}px ${visualizationSettings.getLabelSettings().fontFamily}`;
        context.fillStyle = visualizationSettings.getLabelSettings().textColor;
        context.fillText(text, 10, this.labelFontSize);
        
        context.font = `${this.labelFontSize / 2}px ${visualizationSettings.getLabelSettings().fontFamily}`;
        context.fillStyle = visualizationSettings.getLabelSettings().infoTextColor;
        context.fillText(infoText, 10, this.labelFontSize + 20);

        // Create sprite
        const texture = new THREE.CanvasTexture(canvas);
        const spriteMaterial = new THREE.SpriteMaterial({
            map: texture,
            transparent: true,
            depthWrite: false
        });
        const sprite = new THREE.Sprite(spriteMaterial);
        
        // Scale sprite to maintain readable text size in meters
        const labelScale = visualizationSettings.getLabelSettings().verticalOffset;
        sprite.scale.set(
            (canvas.width / this.labelFontSize) * labelScale,
            (canvas.height / this.labelFontSize) * labelScale,
            1
        );
        sprite.layers.set(NORMAL_LAYER);

        return sprite;
    }

    formatFileSize(size) {
        const units = ['B', 'KB', 'MB', 'GB', 'TB'];
        let i = 0;
        while (size >= 1024 && i < units.length - 1) {
            size /= 1024;
            i++;
        }
        return `${size.toFixed(2)} ${units[i]}`;
    }

    formatAge(lastModified) {
        const now = Date.now();
        const age = now - new Date(lastModified).getTime();
        const days = Math.floor(age / (24 * 60 * 60 * 1000));
        
        if (days < 1) return 'Today';
        if (days === 1) return 'Yesterday';
        if (days < 7) return `${days}d ago`;
        if (days < 30) return `${Math.floor(days / 7)}w ago`;
        if (days < 365) return `${Math.floor(days / 30)}m ago`;
        return `${Math.floor(days / 365)}y ago`;
    }

    formatNodeNameToUrl(nodeName) {
        // Get base URL from environment or default to logseq
        const baseUrl = window.location.origin;
        // Convert node name to lowercase and replace spaces with dashes
        const formattedName = nodeName.toLowerCase().replace(/ /g, '-');
        return `${baseUrl}/#/page/${formattedName}`;
    }

        handleClick(event, isXR = false, intersectedObject = null) {
            let clickedMesh;
    
            if (isXR && intersectedObject) {
                // In XR mode, use the passed intersected object directly
                clickedMesh = intersectedObject;
            } else if (!isXR && event) {
                // Regular mouse click handling
                const rect = event.target.getBoundingClientRect();
                this.mouse.x = ((event.clientX - rect.left) / rect.width) * 2 - 1;
                this.mouse.y = -((event.clientY - rect.top) / rect.height) * 2 + 1;
    
                this.raycaster.setFromCamera(this.mouse, this.camera);
                const intersects = this.raycaster.intersectObjects(Array.from(this.nodeMeshes.values()));
                
                if (intersects.length > 0) {
                    clickedMesh = intersects[0].object;
                }
            }
    
            if (clickedMesh) {
                // Find the clicked node
                const nodeId = Array.from(this.nodeMeshes.entries())
                    .find(([_, mesh]) => mesh === clickedMesh)?.[0];
    
                if (nodeId) {
                    const nodeData = this.nodeData.get(nodeId);
                    if (nodeData) {
                        // Open URL in new tab
                        const url = this.formatNodeNameToUrl(nodeData.label || nodeId);
                        window.open(url, '_blank');
    
                        // Visual feedback
                        const originalEmissive = clickedMesh.material.emissiveIntensity;
                        clickedMesh.material.emissiveIntensity = 2.0;
                        setTimeout(() => {
                            clickedMesh.material.emissiveIntensity = originalEmissive;
                        }, 200);
    
                        // Show XR label if in XR mode
                        if (isXR && this.xrLabelManager) {
                            this.xrLabelManager.showLabel(
                                nodeData.label || nodeId,
                                clickedMesh.position,
                                {
                                    backgroundColor: 'rgba(0, 0, 0, 0.8)',
                                    color: '#ffffff',
                                    font: '24px Arial'
                                }
                            );
                        }
    
                        // Trigger haptic feedback in XR mode
                        if (isXR && window.xrSession) {
                            const inputSource = Array.from(window.xrSession.inputSources).find(source => 
                                source.handedness === 'right' || source.handedness === 'left'
                            );
                            if (inputSource?.gamepad?.hapticActuators?.length > 0) {
                                inputSource.gamepad.hapticActuators[0].pulse(0.5, 100);
                            }
                        }
                    }
                }
            }
        }
    
        initClickHandling(renderer) {
            // Add click event listener to renderer's DOM element
            renderer.domElement.addEventListener('click', this.handleClick);
        }
    
        removeClickHandling(renderer) {
            // Remove click event listener
            renderer.domElement.removeEventListener('click', this.handleClick);
        }

    centerNodes(nodes) {
        if (!nodes || (!Array.isArray(nodes) && typeof nodes !== 'object')) {
            console.warn('Invalid nodes data passed to centerNodes');
            return;
        }

        const nodeArray = Array.isArray(nodes) ? nodes.map((node, index) => {
            if (Array.isArray(node)) {
                return {
                    id: index,
                    x: node[0],
                    y: node[1],
                    z: node[2],
                    vx: node[3],
                    vy: node[4],
                    vz: node[5]
                };
            }
            return node;
        }) : Object.values(nodes);

        if (nodeArray.length === 0) {
            console.warn('Empty nodes array passed to centerNodes');
            return;
        }

        // Calculate center of mass
        let centerX = 0, centerY = 0, centerZ = 0;
        nodeArray.forEach(node => {
            centerX += node.x || 0;
            centerY += node.y || 0;
            centerZ += node.z || 0;
        });
        centerX /= nodeArray.length;
        centerY /= nodeArray.length;
        centerZ /= nodeArray.length;

        // Center around origin
        nodeArray.forEach(node => {
            node.x = (node.x || 0) - centerX;
            node.y = (node.y || 0) - centerY;
            node.z = (node.z || 0) - centerZ;
        });

        // Scale positions to reasonable range in meters
        const maxDist = nodeArray.reduce((max, node) => {
            const dist = Math.sqrt(
                (node.x || 0) * (node.x || 0) + 
                (node.y || 0) * (node.y || 0) + 
                (node.z || 0) * (node.z || 0)
            );
            return Math.max(max, dist);
        }, 0);

        if (maxDist > 0) {
            // Scale to fit in 5 meter radius by default
            const targetRadius = 5.0; // meters
            const scale = targetRadius / maxDist;
            nodeArray.forEach(node => {
                node.x = (node.x || 0) * scale;
                node.y = (node.y || 0) * scale;
                node.z = (node.z || 0) * scale;
            });
        }

        return nodeArray;
    }

        updateNodes(nodes) {
            if (!Array.isArray(nodes)) {
                console.error('updateNodes received invalid nodes:', nodes);
                return;
            }
    
            console.log(`Updating nodes: ${nodes.length}`);
            
            // Center and scale nodes
            const centeredNodes = this.centerNodes(nodes);
            if (!centeredNodes) return;
            
            const existingNodeIds = new Set(centeredNodes.map(node => node.id));
    
            // Remove non-existent nodes
            this.nodeMeshes.forEach((mesh, nodeId) => {
                if (!existingNodeIds.has(nodeId)) {
                    this.scene.remove(mesh);
                    this.nodeMeshes.delete(nodeId);
                    this.nodeData.delete(nodeId);
                    const label = this.nodeLabels.get(nodeId);
                    if (label) {
                        this.scene.remove(label);
                        this.nodeLabels.delete(nodeId);
                    }
                }
            });
    
            // Update or create nodes
            centeredNodes.forEach(node => {
                if (!node.id || typeof node.x !== 'number' || typeof node.y !== 'number' || typeof node.z !== 'number') {
                    console.warn('Invalid node data:', node);
                    return;
                }
    
                // Store node data for click handling
                this.nodeData.set(node.id, node);
    
                const metadata = node.metadata || {};
                const size = this.getNodeSize(metadata);
                const color = this.calculateNodeColor(metadata);
    
                let mesh = this.nodeMeshes.get(node.id);
    
                if (!mesh) {
                    const geometry = this.createNodeGeometry(size, metadata.hyperlink_count || 0);
                    const material = this.createNodeMaterial(color, metadata);
    
                    mesh = new THREE.Mesh(geometry, material);
                    mesh.layers.enable(BLOOM_LAYER);
                    this.scene.add(mesh);
                    this.nodeMeshes.set(node.id, mesh);
    
                    const label = this.createNodeLabel(node.label || node.id, metadata);
                    this.scene.add(label);
                    this.nodeLabels.set(node.id, label);
                } else {
                    mesh.geometry.dispose();
                    mesh.geometry = this.createNodeGeometry(size, metadata.hyperlink_count || 0);
                    mesh.material.dispose();
                    mesh.material = this.createNodeMaterial(color, metadata);
    }

            mesh.position.set(node.x, node.y, node.z);
            const label = this.nodeLabels.get(node.id);
            if (label) {
                label.position.set(node.x, node.y + size + 2, node.z);
            }
        });
    }

    updateEdges(edges) {
        console.log(`Updating edges: ${edges.length}`);
        
        // Create a map of edges with their weights from topic counts
        const edgeWeights = new Map();
        edges.forEach(edge => {
            if (!edge.source || !edge.target_node) {
                console.warn('Invalid edge data:', edge);
                return;
            }

            const edgeKey = `${edge.source}-${edge.target_node}`;
            const weight = edge.weight || 1; // Use provided weight or default to 1
            edgeWeights.set(edgeKey, weight);
        });

        // Remove non-existent edges
        this.edgeMeshes.forEach((line, edgeKey) => {
            if (!edgeWeights.has(edgeKey)) {
                this.scene.remove(line);
                this.edgeMeshes.delete(edgeKey);
            }
        });

        // Update or create edges
        edgeWeights.forEach((weight, edgeKey) => {
            const [source, target] = edgeKey.split('-');
            let line = this.edgeMeshes.get(edgeKey);
            const sourceMesh = this.nodeMeshes.get(source);
            const targetMesh = this.nodeMeshes.get(target);

            if (!line && sourceMesh && targetMesh) {
                const geometry = new THREE.BufferGeometry();
                const positions = new Float32Array(6);
                geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));

                // Scale edge opacity based on weight
                const normalizedWeight = Math.min(weight / 10, 1); // Normalize weight, cap at 1
                const material = new THREE.LineBasicMaterial({
                    color: this.edgeColor,
                    transparent: true,
                    opacity: this.edgeOpacity * normalizedWeight,
                    linewidth: Math.max(1, Math.min(weight, 5)) // Scale line width with weight, between 1-5
                });

                line = new THREE.Line(geometry, material);
                line.layers.set(NORMAL_LAYER);
                this.scene.add(line);
                this.edgeMeshes.set(edgeKey, line);
            }

            if (line && sourceMesh && targetMesh) {
                const positions = line.geometry.attributes.position.array;
                positions[0] = sourceMesh.position.x;
                positions[1] = sourceMesh.position.y;
                positions[2] = sourceMesh.position.z;
                positions[3] = targetMesh.position.x;
                positions[4] = targetMesh.position.y;
                positions[5] = targetMesh.position.z;
                line.geometry.attributes.position.needsUpdate = true;

                // Update edge appearance based on weight
                const normalizedWeight = Math.min(weight / 10, 1);
                line.material.opacity = this.edgeOpacity * normalizedWeight;
                line.material.linewidth = Math.max(1, Math.min(weight, 5));
            }
        });
    }

    updateLabelOrientations(camera) {
        this.nodeLabels.forEach((label, nodeId) => {
            const mesh = this.nodeMeshes.get(nodeId);
            if (mesh) {
                // Position label closer to node due to smaller size
                const size = mesh.geometry.parameters.radius || 
                           mesh.geometry.parameters.width || 
                           this.minNodeSize;
                label.position.set(
                    mesh.position.x,
                    mesh.position.y + size + 0.2, // Reduced offset
                    mesh.position.z
                );
                label.lookAt(camera.position);
            }
        });
    }

    updateFeature(control, value) {
        console.log(`Updating feature: ${control} = ${value}`);
        switch (control) {
            // Node features
            case 'nodeColor':
                if (typeof value === 'number' || typeof value === 'string') {
                    this.nodeColor = new THREE.Color(value);
                    this.nodeMeshes.forEach(mesh => {
                        if (mesh.material) {
                            mesh.material.color.copy(this.nodeColor);
                            mesh.material.emissive.copy(this.nodeColor);
                        }
                    });
                }
                break;
            case 'minNodeSize':
                this.minNodeSize = value; // Value in meters
                break;
            case 'maxNodeSize':
                this.maxNodeSize = value; // Value in meters
                break;
            case 'labelFontSize':
                this.labelFontSize = value;
                break;

            // Edge features
            case 'edgeColor':
                if (typeof value === 'number' || typeof value === 'string') {
                    this.edgeColor = new THREE.Color(value);
                    this.edgeMeshes.forEach(line => {
                        if (line.material) {
                            line.material.color.copy(this.edgeColor);
                        }
                    });
                }
                break;
            case 'edgeOpacity':
                this.edgeOpacity = value;
                this.edgeMeshes.forEach(line => {
                    if (line.material) {
                        line.material.opacity = value;
                    }
                });
                break;
        }
    }

    updateMaterial(settings) {
        console.log('Updating node material settings:', settings);
        
        // Update material settings
        this.materialSettings = {
            ...this.materialSettings,
            metalness: settings.metalness ?? this.materialSettings.metalness,
            roughness: settings.roughness ?? this.materialSettings.roughness,
            clearcoat: settings.clearcoat ?? this.materialSettings.clearcoat,
            clearcoatRoughness: settings.clearcoatRoughness ?? this.materialSettings.clearcoatRoughness,
            opacity: settings.opacity ?? this.materialSettings.opacity,
            emissiveMinIntensity: settings.emissiveMinIntensity ?? this.materialSettings.emissiveMinIntensity,
            emissiveMaxIntensity: settings.emissiveMaxIntensity ?? this.materialSettings.emissiveMaxIntensity
        };

        // Update all existing node materials
        this.nodeMeshes.forEach((mesh, nodeId) => {
            const nodeData = this.nodeData.get(nodeId);
            if (nodeData && mesh.material) {
                // Create new material with updated settings
                mesh.material.dispose(); // Dispose old material
                mesh.material = this.createNodeMaterial(mesh.material.color, nodeData.metadata || {});
            }
        });
    }

    updateEdgesForNode(nodeId) {
        this.edgeMeshes.forEach((line, edgeKey) => {
            const [source, target] = edgeKey.split('-');
            if (source === nodeId || target === nodeId) {
                const positions = line.geometry.attributes.position.array;
                const sourceMesh = this.nodeMeshes.get(source);
                const targetMesh = this.nodeMeshes.get(target);

                if (sourceMesh && targetMesh) {
                    positions[0] = sourceMesh.position.x;
                    positions[1] = sourceMesh.position.y;
                    positions[2] = sourceMesh.position.z;
                    positions[3] = targetMesh.position.x;
                    positions[4] = targetMesh.position.y;
                    positions[5] = targetMesh.position.z;
                    line.geometry.attributes.position.needsUpdate = true;
                }
            }
        });
    }

    getNodePositions() {
        return Array.from(this.nodeMeshes.values()).map(mesh => [
            mesh.position.x,
            mesh.position.y,
            mesh.position.z
        ]);
    }

    dispose() {
        // Dispose node resources
        this.nodeMeshes.forEach(mesh => {
            if (mesh.geometry) mesh.geometry.dispose();
            if (mesh.material) mesh.material.dispose();
            if (mesh.parent) mesh.parent.remove(mesh);
        });

        this.nodeLabels.forEach(label => {
            if (label.material.map) label.material.map.dispose();
            if (label.material) label.material.dispose();
            if (label.parent) label.parent.remove(label);
        });

        // Dispose edge resources
        this.edgeMeshes.forEach(line => {
            if (line.geometry) line.geometry.dispose();
            if (line.material) line.material.dispose();
            if (line.parent) line.parent.remove(line);
        });

        // Clear data maps
        this.nodeMeshes.clear();
        this.nodeLabels.clear();
        this.edgeMeshes.clear();
        this.nodeData.clear();

        // Clean up event listeners
        if (this.renderer) {
            this.removeClickHandling(this.renderer);
        }
    }
}

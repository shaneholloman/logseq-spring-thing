import * as THREE from 'three';
import { MetadataVisualizer } from './MetadataVisualizer';
import { HologramManager } from './HologramManager';
import { NodeMetadata, HologramSettings } from '../types/metadata';
import { XRHand } from '../types/xr';

export class VisualizationController {
    private readonly scene: THREE.Scene;
    private readonly camera: THREE.PerspectiveCamera;
    private readonly renderer: THREE.WebGLRenderer;
    private readonly metadataVisualizer: MetadataVisualizer;
    private readonly hologramManager: HologramManager;
    private clock: THREE.Clock;
    private isXRSession: boolean = false;

    constructor(container: HTMLElement, settings: any) {
        // Initialize Three.js basics
        this.scene = new THREE.Scene();
        this.camera = new THREE.PerspectiveCamera(75, window.innerWidth / window.innerHeight, 0.1, 1000);
        this.renderer = new THREE.WebGLRenderer({
            antialias: true,
            alpha: true,
            logarithmicDepthBuffer: true
        });
        this.renderer.setPixelRatio(window.devicePixelRatio);
        this.renderer.setSize(window.innerWidth, window.innerHeight);
        this.renderer.xr.enabled = true;
        container.appendChild(this.renderer.domElement);

        // Initialize managers
        this.metadataVisualizer = new MetadataVisualizer(this.scene, this.camera, settings);
        this.hologramManager = new HologramManager(this.scene, settings.hologram);
        this.clock = new THREE.Clock();

        // Set up XR session change handling
        this.renderer.xr.addEventListener('sessionstart', () => {
            this.isXRSession = true;
            this.hologramManager.setXRMode(true);
        });

        this.renderer.xr.addEventListener('sessionend', () => {
            this.isXRSession = false;
            this.hologramManager.setXRMode(false);
        });

        // Set up window resize handling
        window.addEventListener('resize', this.onWindowResize.bind(this));
    }

    private onWindowResize(): void {
        this.camera.aspect = window.innerWidth / window.innerHeight;
        this.camera.updateProjectionMatrix();
        this.renderer.setSize(window.innerWidth, window.innerHeight);
    }

    public updateNodes(nodes: NodeMetadata[]): void {
        // Clear existing nodes
        this.scene.children
            .filter(child => child.userData.isNode)
            .forEach(node => this.scene.remove(node));

        // Create new nodes
        nodes.forEach(metadata => {
            const nodeMesh = this.metadataVisualizer.createNodeVisual(metadata);
            nodeMesh.userData.isNode = true;
            nodeMesh.position.set(
                metadata.position.x,
                metadata.position.y,
                metadata.position.z
            );
            this.scene.add(nodeMesh);
        });
    }

    public updateHologramSettings(settings: Partial<HologramSettings>): void {
        this.hologramManager.updateSettings(settings);
    }

    public handleHandInput(hand: XRHand): void {
        if (this.isXRSession) {
            this.hologramManager.handleHandInteraction(hand);
        }
    }

    public animate(): void {
        const render = () => {
            const delta = this.clock.getDelta();
            
            // Update hologram animations
            this.hologramManager.update(delta);

            // Render scene
            this.renderer.render(this.scene, this.camera);
        };

        this.renderer.setAnimationLoop(render);
    }

    public dispose(): void {
        // Clean up event listeners
        window.removeEventListener('resize', this.onWindowResize.bind(this));

        // Dispose managers
        this.metadataVisualizer.dispose();
        this.hologramManager.dispose();

        // Stop animation loop
        this.renderer.setAnimationLoop(null);

        // Dispose Three.js resources
        this.renderer.dispose();
        this.scene.traverse((object) => {
            if (object instanceof THREE.Mesh) {
                object.geometry.dispose();
                if (Array.isArray(object.material)) {
                    object.material.forEach(material => material.dispose());
                } else {
                    object.material.dispose();
                }
            }
        });
    }
}

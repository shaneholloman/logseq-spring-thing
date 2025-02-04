import { BufferGeometry, SphereGeometry, CylinderGeometry } from 'three';

export class GeometryFactory {
    private static instance: GeometryFactory;
    private geometryCache = new Map<string, BufferGeometry>();

    private constructor() {}

    static getInstance(): GeometryFactory {
        if (!GeometryFactory.instance) {
            GeometryFactory.instance = new GeometryFactory();
        }
        return GeometryFactory.instance;
    }

    getNodeGeometry(quality: 'low' | 'medium' | 'high', context: 'ar' | 'desktop' = 'desktop'): BufferGeometry {
        const cacheKey = `node-${quality}-${context}`;
        if (this.geometryCache.has(cacheKey)) {
            return this.geometryCache.get(cacheKey)!;
        }

        let geometry: BufferGeometry;
        
        if (context === 'ar') {
            // AR (Meta Quest) - Optimized geometry
            switch (quality) {
                case 'high':
                    geometry = new SphereGeometry(1, 16, 12); // Reduced from 32
                    break;
                case 'medium':
                    geometry = new SphereGeometry(1, 12, 8);  // Further reduced
                    break;
                case 'low':
                    geometry = new SphereGeometry(1, 8, 6);   // Minimal
                    break;
            }
        } else {
            // Desktop/VR - Full quality geometry
            const segments = {
                low: 16,
                medium: 24,
                high: 32
            }[quality] || 24;
            
            geometry = new SphereGeometry(1, segments, segments);
        }

        this.geometryCache.set(cacheKey, geometry);
        return geometry;
    }

    getHologramGeometry(type: string, quality: string): BufferGeometry {
        const cacheKey = `hologram-${type}-${quality}`;
        if (this.geometryCache.has(cacheKey)) {
            return this.geometryCache.get(cacheKey)!;
        }

        const segments = {
            low: { ring: 64, sphere: 32 },
            medium: { ring: 96, sphere: 48 },
            high: { ring: 128, sphere: 64 }
        }[quality] || { ring: 96, sphere: 48 };

        let geometry: BufferGeometry;
        switch (type) {
            case 'ring':
                // Translucent rings at scene scale
                geometry = new SphereGeometry(40, segments.ring, segments.ring);
                break;
            case 'outerSphere':
                geometry = new SphereGeometry(200, segments.sphere, segments.sphere);
                break;
            case 'middleSphere':
                geometry = new SphereGeometry(100, segments.sphere, segments.sphere);
                break;
            case 'innerSphere':
                geometry = new SphereGeometry(40, segments.sphere, segments.sphere);
                break;
            default:
                geometry = new SphereGeometry(40, segments.sphere, segments.sphere);
        }

        this.geometryCache.set(cacheKey, geometry);
        return geometry;
    }

    getEdgeGeometry(context: 'ar' | 'desktop' = 'desktop'): BufferGeometry {
        const cacheKey = `edge-${context}`;
        if (this.geometryCache.has(cacheKey)) {
            return this.geometryCache.get(cacheKey)!;
        }

        // CylinderGeometry parameters:
        // radiusTop, radiusBottom, height, radialSegments
        const radialSegments = context === 'ar' ? 4 : 8; // Reduce segments in AR
        const geometry = new CylinderGeometry(0.05, 0.05, 1, radialSegments);
        geometry.rotateX(Math.PI / 2); // Align with direction of travel
        
        this.geometryCache.set(cacheKey, geometry);
        return geometry;
    }

    dispose(): void {
        this.geometryCache.forEach(geometry => geometry.dispose());
        this.geometryCache.clear();
    }
}

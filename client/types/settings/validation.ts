import { Settings } from '../settings';

export interface ValidationError {
    path: string;
    message: string;
}

export interface ValidationResult {
    isValid: boolean;
    errors: ValidationError[];
}

export function validateSettings(settings: Settings): ValidationResult {
    const errors: ValidationError[] = [];

    // Validate visualization settings
    if (settings.visualization) {
        validateVisualizationSettings(settings.visualization, errors);
    }

    // Validate system settings
    if (settings.system) {
        validateSystemSettings(settings.system, errors);
    }

    // Validate XR settings
    if (settings.xr) {
        validateXRSettings(settings.xr, errors);
    }

    return {
        isValid: errors.length === 0,
        errors
    };
}

function validateVisualizationSettings(visualization: Settings['visualization'], errors: ValidationError[]): void {
    // Validate bloom settings
    if (visualization.bloom?.enabled) {
        validateNumericRange('visualization.bloom.strength', visualization.bloom.strength, 0, 3, errors);
        validateNumericRange('visualization.bloom.radius', visualization.bloom.radius, 0, 2, errors);
        validateNumericRange('visualization.bloom.threshold', visualization.bloom.threshold, 0, 1.5, errors);
    }

    // Validate hologram settings
    if (visualization.hologram) {
        if (visualization.hologram.enableBuckminster) {
            validateNumericRange('visualization.hologram.buckminsterScale', visualization.hologram.buckminsterScale, 0.1, 50, errors);
            validateNumericRange('visualization.hologram.buckminsterOpacity', visualization.hologram.buckminsterOpacity, 0, 1, errors);
        }
        if (visualization.hologram.enableGeodesic) {
            validateNumericRange('visualization.hologram.geodesicScale', visualization.hologram.geodesicScale, 0.1, 50, errors);
            validateNumericRange('visualization.hologram.geodesicOpacity', visualization.hologram.geodesicOpacity, 0, 1, errors);
        }
    }

    // Validate physics settings
    if (visualization.physics?.enabled) {
        // Align with force_calculation.wgsl SimulationParams ranges
        validateNumericRange('visualization.physics.attractionStrength', visualization.physics.attractionStrength, 0.001, 1.0, errors);
        validateNumericRange('visualization.physics.repulsionStrength', visualization.physics.repulsionStrength, 1.0, 10000.0, errors);
        validateNumericRange('visualization.physics.springStrength', visualization.physics.springStrength, 0.001, 1.0, errors);
        validateNumericRange('visualization.physics.damping', visualization.physics.damping, 0.5, 0.95, errors);
        validateNumericRange('visualization.physics.iterations', visualization.physics.iterations, 1, 500, errors);
        validateNumericRange('visualization.physics.maxVelocity', visualization.physics.maxVelocity, 0.1, 1.0, errors);
        validateNumericRange('visualization.physics.collisionRadius', visualization.physics.collisionRadius, 0.1, 1.0, errors);
        validateNumericRange('visualization.physics.boundsSize', visualization.physics.boundsSize, 10.0, 500.0, errors);
    }

    // Validate node settings
    if (visualization.nodes) {
        validateQualityEnum('visualization.nodes.quality', visualization.nodes.quality, errors);
        validateNumericRange('visualization.nodes.baseSize', visualization.nodes.baseSize, 0.5, 10.0, errors);
        validateNumericRange('visualization.nodes.opacity', visualization.nodes.opacity, 0.3, 1, errors);
        validateNumericRange('visualization.nodes.metalness', visualization.nodes.metalness, 0.2, 0.8, errors);
        validateNumericRange('visualization.nodes.roughness', visualization.nodes.roughness, 0.2, 0.8, errors);
        
        // Validate size range array
        if (visualization.nodes.sizeRange) {
            validateNumericRange('visualization.nodes.sizeRange[0]', visualization.nodes.sizeRange[0], 0.5, 5.0, errors);
            validateNumericRange('visualization.nodes.sizeRange[1]', visualization.nodes.sizeRange[1], 1.0, 10.0, errors);
            if (visualization.nodes.sizeRange[0] >= visualization.nodes.sizeRange[1]) {
                errors.push({
                    path: 'visualization.nodes.sizeRange',
                    message: 'Min size must be less than max size'
                });
            }
        }
    }
}

function validateSystemSettings(system: Settings['system'], errors: ValidationError[]): void {
    // Validate websocket settings
    if (system.websocket) {
        validateNumericRange('system.websocket.updateRate', system.websocket.updateRate, 1, 120, errors);
        validateNumericRange('system.websocket.reconnectAttempts', system.websocket.reconnectAttempts, 1, 10, errors);
        validateNumericRange('system.websocket.reconnectDelay', system.websocket.reconnectDelay, 1000, 30000, errors);
        validateNumericRange('system.websocket.binaryChunkSize', system.websocket.binaryChunkSize, 1024, 1048576, errors);
        validateNumericRange('system.websocket.compressionThreshold', system.websocket.compressionThreshold, 512, 1048576, errors);
    }
}

function validateXRSettings(xr: Settings['xr'], errors: ValidationError[]): void {
    // Validate mode
    if (!['immersive-ar', 'immersive-vr'].includes(xr.mode)) {
        errors.push({
            path: 'xr.mode',
            message: 'XR mode must be either immersive-ar or immersive-vr'
        });
    }

    // Validate space type
    if (!['viewer', 'local', 'local-floor', 'bounded-floor', 'unbounded'].includes(xr.spaceType)) {
        errors.push({
            path: 'xr.spaceType',
            message: 'Invalid space type'
        });
    }

    // Validate quality
    validateQualityEnum('xr.quality', xr.quality, errors);

    // Validate hand tracking settings
    validateNumericRange('xr.handMeshOpacity', xr.handMeshOpacity, 0.1, 0.8, errors);
    validateNumericRange('xr.handPointSize', xr.handPointSize, 1, 10, errors);
    validateNumericRange('xr.handRayWidth', xr.handRayWidth, 0.5, 5, errors);
    validateNumericRange('xr.gestureSmoothing', xr.gestureSmoothing, 0.3, 0.9, errors);

    // Validate interaction settings with refined ranges
    validateNumericRange('xr.hapticIntensity', xr.hapticIntensity, 0.1, 0.8, errors);
    validateNumericRange('xr.dragThreshold', xr.dragThreshold, 0.05, 0.3, errors);
    validateNumericRange('xr.pinchThreshold', xr.pinchThreshold, 0.1, 0.5, errors);
    validateNumericRange('xr.rotationThreshold', xr.rotationThreshold, 0.05, 0.3, errors);
    validateNumericRange('xr.interactionRadius', xr.interactionRadius, 0.1, 0.5, errors);
    validateNumericRange('xr.movementSpeed', xr.movementSpeed, 0.05, 0.2, errors);
    validateNumericRange('xr.deadZone', xr.deadZone, 0.05, 0.2, errors);

    // Validate movement axes
    if (xr.movementAxes) {
        validateNumericRange('xr.movementAxes.horizontal', xr.movementAxes.horizontal, 0, 5, errors);
        validateNumericRange('xr.movementAxes.vertical', xr.movementAxes.vertical, 0, 5, errors);
    }

    // Validate scene understanding settings with refined ranges
    validateNumericRange('xr.planeOpacity', xr.planeOpacity, 0.1, 0.5, errors);
    validateNumericRange('xr.planeDetectionDistance', xr.planeDetectionDistance ?? 3.0, 1.0, 10.0, errors);

    // Validate passthrough settings with refined ranges
    validateNumericRange('xr.passthroughOpacity', xr.passthroughOpacity, 0.5, 1.0, errors);
    validateNumericRange('xr.passthroughBrightness', xr.passthroughBrightness, 0.8, 1.5, errors);
    validateNumericRange('xr.passthroughContrast', xr.passthroughContrast, 0.8, 1.5, errors);
    validateNumericRange('xr.portalSize', xr.portalSize, 1.0, 5.0, errors);
    validateNumericRange('xr.portalEdgeWidth', xr.portalEdgeWidth, 0.5, 3.0, errors);

    // Validate that portal edge width is proportional to portal size
    if (xr.portalEdgeWidth && xr.portalSize && xr.portalEdgeWidth > xr.portalSize * 0.2) {
        errors.push({
            path: 'xr.portalEdgeWidth',
            message: 'Portal edge width should not exceed 20% of portal size'
        });
    }
}

function validateNumericRange(path: string, value: number, min: number, max: number, errors: ValidationError[]): void {
    if (value === undefined || value === null) {
        return; // Skip validation for undefined/null values
    }

    if (typeof value !== 'number' || isNaN(value)) {
        errors.push({
            path,
            message: `${path} must be a number`
        });
        return;
    }

    if (value < min || value > max) {
        errors.push({
            path,
            message: `${path} must be between ${min} and ${max}`
        });
    }
}

function validateQualityEnum(path: string, value: string, errors: ValidationError[]): void {
    if (!['low', 'medium', 'high'].includes(value)) {
        errors.push({
            path,
            message: `${path} must be one of: low, medium, high`
        });
    }
}

export function validateSettingValue(path: string, value: unknown, settings: Settings): ValidationError[] {
    // Create a copy of settings and update with new value
    const settingsCopy = JSON.parse(JSON.stringify(settings));
    const parts = path.split('.');
    let current = settingsCopy;
    for (let i = 0; i < parts.length - 1; i++) {
        current = current[parts[i]];
    }
    current[parts[parts.length - 1]] = value;

    // Validate the entire settings object
    const result = validateSettings(settingsCopy);
    
    // Filter errors to only include those related to the changed path
    // or any interdependent settings that might be affected
    return result.errors.filter(error => 
        error.path === path || isInterdependentSetting(path, error.path)
    );
}

function isInterdependentSetting(changedPath: string, errorPath: string): boolean {
    // Define interdependencies between settings
    const interdependencies: Record<string, string[]> = {
        'visualization.bloom.enabled': [
            'visualization.rendering.enableAntialiasing',
            'visualization.bloom.strength',
            'visualization.bloom.radius'
        ],
        'visualization.physics.enabled': [
            'visualization.animations.enableNodeAnimations',
            'visualization.physics.attractionStrength',
            'visualization.physics.repulsionStrength'
        ],
        'xr.enableHandTracking': [
            'xr.handMeshEnabled',
            'xr.handRayEnabled',
            'xr.gestureSmoothing'
        ],
        'xr.enablePassthroughPortal': [
            'xr.passthroughOpacity',
            'xr.passthroughBrightness',
            'xr.passthroughContrast',
            'xr.portalSize',
            'xr.portalEdgeWidth'
        ]
    };

    // Check if the error path is interdependent with the changed path
    return interdependencies[changedPath]?.includes(errorPath) || false;
}
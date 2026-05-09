import { describe, it, expect } from 'vitest';
import {
  QUALITY_PRESETS,
  getPresetById,
  getRecommendedPreset,
  validatePresetSettings,
  QualityPreset,
} from '../qualityPresets';

describe('qualityPresets', () => {
  // ---- QUALITY_PRESETS data integrity ----

  describe('QUALITY_PRESETS', () => {
    it('should contain exactly 4 presets', () => {
      expect(QUALITY_PRESETS).toHaveLength(4);
    });

    it('should have unique ids', () => {
      const ids = QUALITY_PRESETS.map((p) => p.id);
      expect(new Set(ids).size).toBe(ids.length);
    });

    it('should cover all category tiers', () => {
      const categories = QUALITY_PRESETS.map((p) => p.category);
      expect(categories).toContain('performance');
      expect(categories).toContain('balanced');
      expect(categories).toContain('quality');
      expect(categories).toContain('ultra');
    });

    it('each preset should have required fields', () => {
      for (const preset of QUALITY_PRESETS) {
        expect(preset.id).toBeTruthy();
        expect(preset.name).toBeTruthy();
        expect(preset.description).toBeTruthy();
        expect(preset.icon).toBeTruthy();
        expect(typeof preset.settings).toBe('object');
        expect(Object.keys(preset.settings).length).toBeGreaterThan(0);
      }
    });

    it('physics iterations should increase with quality tier', () => {
      const iterKey = 'visualisation.graphs.logseq.physics.iterations';
      const low = QUALITY_PRESETS.find((p) => p.id === 'low')!;
      const medium = QUALITY_PRESETS.find((p) => p.id === 'medium')!;
      const high = QUALITY_PRESETS.find((p) => p.id === 'high')!;
      const ultra = QUALITY_PRESETS.find((p) => p.id === 'ultra')!;

      expect(low.settings[iterKey]).toBeLessThan(medium.settings[iterKey]);
      expect(medium.settings[iterKey]).toBeLessThan(high.settings[iterKey]);
      expect(high.settings[iterKey]).toBeLessThan(ultra.settings[iterKey]);
    });

    it('system requirements RAM should increase with quality tier', () => {
      const rams = QUALITY_PRESETS.map((p) => p.systemRequirements!.minRAM!);
      for (let i = 1; i < rams.length; i++) {
        expect(rams[i]).toBeGreaterThanOrEqual(rams[i - 1]);
      }
    });
  });

  // ---- getPresetById ----

  describe('getPresetById', () => {
    it('should return the correct preset for known id', () => {
      const preset = getPresetById('low');
      expect(preset).toBeDefined();
      expect(preset!.id).toBe('low');
      expect(preset!.category).toBe('performance');
    });

    it('should return undefined for unknown id', () => {
      expect(getPresetById('nonexistent')).toBeUndefined();
    });

    it('should return undefined for empty string', () => {
      expect(getPresetById('')).toBeUndefined();
    });

    it('should find each preset by its id', () => {
      for (const preset of QUALITY_PRESETS) {
        const found = getPresetById(preset.id);
        expect(found).toBe(preset);
      }
    });
  });

  // ---- getRecommendedPreset ----

  describe('getRecommendedPreset', () => {
    it('should recommend ultra for high-end systems', () => {
      const preset = getRecommendedPreset({ ram: 64, vram: 12, gpu: 'RTX 4090' });
      expect(preset.id).toBe('ultra');
    });

    it('should recommend high for mid-high systems', () => {
      const preset = getRecommendedPreset({ ram: 16, vram: 6, gpu: 'RTX 2070' });
      expect(preset.id).toBe('high');
    });

    it('should recommend medium for moderate systems', () => {
      const preset = getRecommendedPreset({ ram: 8, vram: 2, gpu: 'GTX 1060' });
      expect(preset.id).toBe('medium');
    });

    it('should recommend low for entry-level systems', () => {
      const preset = getRecommendedPreset({ ram: 4, vram: 1, gpu: 'Intel UHD' });
      expect(preset.id).toBe('low');
    });

    it('should recommend low when vram is 0', () => {
      const preset = getRecommendedPreset({ ram: 16, vram: 0, gpu: 'Integrated' });
      expect(preset.id).toBe('low');
    });

    it('should handle boundary values (exactly matching threshold)', () => {
      // Exactly 32 RAM + 8 VRAM = ultra threshold
      const preset = getRecommendedPreset({ ram: 32, vram: 8, gpu: 'RTX 3080' });
      expect(preset.id).toBe('ultra');
    });

    it('should prioritize vram over ram for tier selection', () => {
      // 64 GB RAM but only 1 GB VRAM should not yield ultra
      const preset = getRecommendedPreset({ ram: 64, vram: 1, gpu: 'Low VRAM' });
      expect(preset.id).toBe('low');
    });
  });

  // ---- validatePresetSettings ----

  describe('validatePresetSettings', () => {
    it('should return true for valid settings object', () => {
      expect(validatePresetSettings({ key: 'value' })).toBe(true);
    });

    it('should return true for empty object', () => {
      expect(validatePresetSettings({})).toBe(true);
    });

    it('should return false for null', () => {
      expect(validatePresetSettings(null as any)).toBe(false);
    });

    it('should return false for non-object types', () => {
      expect(validatePresetSettings('string' as any)).toBe(false);
      expect(validatePresetSettings(42 as any)).toBe(false);
    });

    it('should return true for all built-in preset settings', () => {
      for (const preset of QUALITY_PRESETS) {
        expect(validatePresetSettings(preset.settings)).toBe(true);
      }
    });
  });
});

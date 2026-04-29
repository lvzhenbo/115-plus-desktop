import JASSUB from 'jassub';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import workerUrl from 'jassub/dist/worker/worker.js?worker&url';
import wasmUrl from 'jassub/dist/wasm/jassub-worker.wasm?url';
import modernWasmUrl from 'jassub/dist/wasm/jassub-worker-modern.wasm?url';
import type { ParsedAssSubtitleTrack } from './types';
import { normalizeFontFamily } from './font';

interface SystemFontSource {
  familyNames: string[];
  path: string;
}

interface SystemFontConfig {
  defaultFont: string;
  fonts: SystemFontSource[];
  unmatchedFamilies?: string[];
}

const systemFontConfigPromises = new Map<string, Promise<SystemFontConfig>>();

function buildFontConfigKey(fontFamilies: string[]) {
  return [...new Set(fontFamilies.map((f) => normalizeFontFamily(f).toLowerCase()).filter(Boolean))]
    .sort()
    .join('\u0000');
}

async function loadSystemFontConfig(fontFamilies: string[]) {
  const resolvedFontFamilies: string[] = [];
  const seenFontFamilies = new Set<string>();

  for (const fontFamily of fontFamilies) {
    const normalized = normalizeFontFamily(fontFamily);
    const key = normalized.toLowerCase();
    if (!normalized || seenFontFamilies.has(key)) continue;

    seenFontFamilies.add(key);
    resolvedFontFamilies.push(normalized);
  }

  const cacheKey = buildFontConfigKey(resolvedFontFamilies);

  let promise = systemFontConfigPromises.get(cacheKey);
  if (!promise) {
    promise = invoke<SystemFontConfig>('subtitle_get_system_font_config', {
      fontFamilies: resolvedFontFamilies,
    });
    systemFontConfigPromises.set(cacheKey, promise);
  }

  return promise;
}

function fontSourceToUrl(source: SystemFontSource): string {
  return convertFileSrc(source.path);
}

function buildJassubFontSources(config: SystemFontConfig) {
  const fonts: string[] = [];
  const availableFonts: Record<string, string> = {};
  const loadedPaths = new Set<string>();

  for (const source of config.fonts) {
    const url = fontSourceToUrl(source);
    fonts.push(url);
    for (const name of source.familyNames) {
      availableFonts[name] = url;
    }
    loadedPaths.add(source.path);
  }

  return { fonts, availableFonts, loadedPaths };
}

function collectPendingFontUrls(config: SystemFontConfig, loadedPaths: Set<string>): string[] {
  const pending: string[] = [];

  for (const source of config.fonts) {
    if (loadedPaths.has(source.path)) continue;
    loadedPaths.add(source.path);
    pending.push(fontSourceToUrl(source));
  }

  return pending;
}

export class AssSubtitleRenderer {
  private instance: JASSUB | null = null;
  private boundVideoElement: HTMLVideoElement | null = null;
  private loadedFontPaths = new Set<string>();
  private defaultFont: string | null = null;
  private reportedUnmatchedFamilies = new Set<string>();

  constructor(
    private readonly resolveVideoElement: () => HTMLVideoElement | null,
    private readonly resolveCanvasElement: () => HTMLCanvasElement | null,
    private readonly onUnmatchedFonts?: (families: string[]) => void,
  ) {}

  private createInstance(
    videoElement: HTMLVideoElement,
    canvasElement: HTMLCanvasElement,
    track: ParsedAssSubtitleTrack,
    systemFontConfig: SystemFontConfig,
  ) {
    if (systemFontConfig.fonts.length === 0) {
      throw new Error('未找到可用的系统字幕字体');
    }

    const fontSources = buildJassubFontSources(systemFontConfig);

    this.instance = new JASSUB({
      video: videoElement,
      canvas: canvasElement,
      subContent: track.content,
      fonts: fontSources.fonts,
      availableFonts: fontSources.availableFonts,
      defaultFont: systemFontConfig.defaultFont,
      queryFonts: false,
      workerUrl,
      wasmUrl,
      modernWasmUrl,
    });
    this.boundVideoElement = videoElement;
    this.loadedFontPaths = fontSources.loadedPaths;
    this.defaultFont = systemFontConfig.defaultFont;
  }

  private async updateFontsIfNeeded(systemFontConfig: SystemFontConfig) {
    const instance = this.instance!;

    const pendingUrls = collectPendingFontUrls(systemFontConfig, this.loadedFontPaths);
    if (pendingUrls.length > 0) {
      await instance.renderer.addFonts(pendingUrls);
    }

    if (this.defaultFont !== systemFontConfig.defaultFont) {
      await instance.renderer.setDefaultFont(systemFontConfig.defaultFont);
      this.defaultFont = systemFontConfig.defaultFont;
    }
  }

  private async updateVideoElementIfNeeded(videoElement: HTMLVideoElement) {
    if (videoElement === this.boundVideoElement) return;
    await this.instance!.setVideo(videoElement);
    this.boundVideoElement = videoElement;
  }

  private async ensureInstance(track: ParsedAssSubtitleTrack) {
    const videoElement = this.resolveVideoElement();
    const canvasElement = this.resolveCanvasElement();

    if (!videoElement || !canvasElement) {
      throw new Error('ASS 渲染器初始化失败，视频元素或画布不可用');
    }

    const systemFontConfig = await loadSystemFontConfig(track.fontFamilies);

    if (systemFontConfig.unmatchedFamilies?.length && this.onUnmatchedFonts) {
      const newUnmatched = systemFontConfig.unmatchedFamilies.filter(
        (f) => !this.reportedUnmatchedFamilies.has(f),
      );
      if (newUnmatched.length > 0) {
        for (const f of newUnmatched) {
          this.reportedUnmatchedFamilies.add(f);
        }
        this.onUnmatchedFonts(newUnmatched);
      }
    }

    let created = false;

    if (!this.instance) {
      this.createInstance(videoElement, canvasElement, track, systemFontConfig);
      created = true;
    }

    const instance = this.instance!;
    await instance.ready;

    if (!created) {
      await this.updateFontsIfNeeded(systemFontConfig);
    }

    await this.updateVideoElementIfNeeded(videoElement);

    return { instance, created };
  }

  async loadTrack(track: ParsedAssSubtitleTrack) {
    const { instance, created } = await this.ensureInstance(track);

    if (!created) {
      await instance.renderer.setTrack(track.content);
    }

    this.setVisible(true);
  }

  async clearTrack() {
    if (!this.instance) {
      this.setVisible(false);
      return;
    }

    await this.instance.ready;
    await this.instance.renderer.freeTrack();
    this.setVisible(false);
  }

  setVisible(visible: boolean) {
    const canvasElement = this.resolveCanvasElement();
    if (!canvasElement) return;

    canvasElement.style.visibility = visible ? 'visible' : 'hidden';
  }

  async destroy() {
    if (!this.instance) return;

    const instance = this.instance;
    this.instance = null;
    this.boundVideoElement = null;
    this.loadedFontPaths = new Set();
    this.defaultFont = null;
    this.reportedUnmatchedFamilies = new Set();
    await instance.destroy();
  }
}

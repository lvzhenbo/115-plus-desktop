import type {
  Loader,
  LoaderContext,
  LoaderConfiguration,
  LoaderCallbacks,
  LoaderStats,
  LoaderResponse,
} from 'hls.js';
import { fetch } from '@tauri-apps/plugin-http';

export default class CustomLoader implements Loader<LoaderContext> {
  context: LoaderContext | null = null;
  stats: LoaderStats = this.createDefaultStats();

  private callbacks: LoaderCallbacks<LoaderContext> | null = null;
  private abortController: AbortController | null = null;
  private timeoutId: ReturnType<typeof setTimeout> | null = null;

  constructor(_config?: any) {}

  private createDefaultStats(): LoaderStats {
    return {
      aborted: false,
      loaded: 0,
      retry: 0,
      total: 0,
      chunkCount: 0,
      bwEstimate: 0,
      loading: { start: 0, first: 0, end: 0 },
      parsing: { start: 0, end: 0 },
      buffering: { start: 0, first: 0, end: 0 },
    };
  }

  destroy(): void {
    this.abort();
    this.context = null;
    this.callbacks = null;
  }

  abort(): void {
    this.clearTimeout();
    if (this.abortController) {
      this.abortController.abort();
      this.abortController = null;
    }
    this.stats.aborted = true;
  }

  private clearTimeout(): void {
    if (this.timeoutId !== null) {
      clearTimeout(this.timeoutId);
      this.timeoutId = null;
    }
  }

  load(
    context: LoaderContext,
    _config: LoaderConfiguration,
    callbacks: LoaderCallbacks<LoaderContext>,
  ): void {
    this.context = context;
    this.callbacks = callbacks;
    this.stats = this.createDefaultStats();
    this.stats.loading.start = performance.now();

    this.abortController = new AbortController();

    // 设置超时（30秒）
    this.timeoutId = setTimeout(() => {
      if (!this.stats.aborted) {
        this.abort();
        this.onError(0, 'Request timeout');
      }
    }, 30000);

    fetch(context.url, {
      headers: context.headers || {},
      signal: this.abortController.signal,
    })
      .then(async (response) => {
        this.clearTimeout();
        if (this.stats.aborted) return;

        const endTime = performance.now();
        this.stats.loading.first = this.stats.loading.first || endTime;
        this.stats.loading.end = endTime;

        if (!response.ok) {
          this.onError(response.status, response.statusText);
          return;
        }

        let responseData: string | ArrayBuffer;
        if (context.responseType === 'text' || context.responseType === 'json') {
          responseData = await response.text();
        } else {
          responseData = await response.arrayBuffer();
        }

        this.stats.loaded =
          typeof responseData === 'string' ? responseData.length : responseData.byteLength;
        this.stats.total = this.stats.loaded;

        const loaderResponse: LoaderResponse = {
          url: context.url,
          data: responseData,
        };

        if (this.callbacks?.onSuccess) {
          this.callbacks.onSuccess(loaderResponse, this.stats, context, response);
        }
      })
      .catch((error) => {
        this.clearTimeout();
        // 已主动中断时不触发错误回调（abort 方法中已处理或已由超时处理）
        if (this.stats.aborted) return;
        this.onError(0, error.message || 'Network Error');
      });
  }

  getCacheAge(): number | null {
    return null;
  }

  getResponseHeader(_name: string): string | null {
    return null;
  }

  private onError(code: number, text: string): void {
    if (this.callbacks?.onError && this.context) {
      this.callbacks.onError({ code, text }, this.context, null, this.stats);
    }
  }
}

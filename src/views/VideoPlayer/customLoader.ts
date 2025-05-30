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
  stats: LoaderStats = {
    aborted: false,
    loaded: 0,
    retry: 0,
    total: 0,
    chunkCount: 0,
    bwEstimate: 0,
    loading: {
      start: 0,
      first: 0,
      end: 0,
    },
    parsing: {
      start: 0,
      end: 0,
    },
    buffering: {
      start: 0,
      first: 0,
      end: 0,
    },
  };

  private callbacks: LoaderCallbacks<LoaderContext> | null = null;
  private abortController: AbortController | null = null;

  constructor(_config?: any) {
    // 配置初始化，如果需要的话
  }

  destroy(): void {
    this.abort();
    this.context = null;
    this.callbacks = null;
  }

  abort(): void {
    if (this.abortController) {
      this.abortController.abort();
      this.abortController = null;
    }
    this.stats.aborted = true;
  }

  load(
    context: LoaderContext,
    _config: LoaderConfiguration,
    callbacks: LoaderCallbacks<LoaderContext>,
  ): void {
    this.context = context;
    this.callbacks = callbacks;
    this.stats.aborted = false;
    this.stats.loaded = 0;
    this.stats.retry = 0;
    this.stats.loading.start = performance.now();

    this.abortController = new AbortController();

    // 使用 Tauri 的 fetch 进行请求
    fetch(context.url, {
      headers: context.headers || {},
      signal: this.abortController.signal,
    })
      .then(async (response) => {
        if (this.stats.aborted) {
          return;
        }

        const endTime = performance.now();
        this.stats.loading.end = endTime;
        this.stats.loading.first = endTime;

        if (!response.ok) {
          this.onError(response.status, response.statusText);
          return;
        }

        // 根据响应类型获取数据
        let responseData: string | ArrayBuffer;
        if (context.responseType === 'text' || context.responseType === 'json') {
          responseData = await response.text();
        } else {
          responseData = await response.arrayBuffer();
        }

        // 更新统计信息
        this.stats.loaded =
          typeof responseData === 'string' ? responseData.length : responseData.byteLength;
        this.stats.total = this.stats.loaded;

        // 构建响应对象
        const loaderResponse: LoaderResponse = {
          url: context.url,
          data: responseData,
        };

        // 调用成功回调
        if (this.callbacks?.onSuccess) {
          this.callbacks.onSuccess(loaderResponse, this.stats, context, response);
        }
      })
      .catch((error) => {
        if (this.stats.aborted) {
          return;
        }

        console.error('Custom loader error:', error);
        this.onError(0, error.message || 'Network Error');
      });
  }

  getCacheAge(): number | null {
    return null;
  }

  getResponseHeader(_name: string): string | null {
    // 在实际实现中，您可能需要存储 response headers
    // 这里返回 null 作为简单实现
    return null;
  }

  private onError(code: number, text: string): void {
    if (this.callbacks?.onError && this.context) {
      this.callbacks.onError(
        { code, text },
        this.context,
        null, // networkDetails
        this.stats,
      );
    }
  }
}

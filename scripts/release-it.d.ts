/**
 * 扩充 release-it 官方类型声明。
 * 官方 types/index.d.ts 仅导出 Config，不包含 Plugin 类。
 * 以下声明基于 release-it 实际源码（lib/plugin/Plugin.js / lib/log.js / lib/config.js）。
 */
declare module 'release-it' {
  // ---- Log (lib/log.js) ----

  interface Log {
    /** 操作日志，dry-run 时内部已处理输出格式 */
    exec(...args: unknown[]): void;
    verbose(...args: unknown[]): void;
    warn(...args: unknown[]): void;
    info(...args: unknown[]): void;
    error(...args: unknown[]): void;
    log(...args: unknown[]): void;
    obtrusive(...args: unknown[]): void;
    preview(options: { title: string; text: string }): void;
  }

  // ---- Config (lib/config.js) ----

  interface Config {
    readonly isDryRun: boolean;
    readonly isCI: boolean;

    getContext(): Record<string, unknown>;
    getContext(path: string): unknown;
    setContext(options: Record<string, unknown>): void;
  }

  // ---- Shell (lib/shell.js) ----

  interface ShellExecOptions {
    write?: boolean;
    external?: boolean;
  }

  // ---- Plugin (lib/plugin/Plugin.js) ----

  class Plugin {
    readonly namespace: string;
    readonly options: Record<string, unknown>;
    readonly config: Config;
    readonly log: Log;

    constructor(args: {
      namespace: string;
      options?: Record<string, unknown>;
      container?: {
        config: Config;
        log: Log;
        shell?: unknown;
        spinner?: unknown;
        prompt?: unknown;
      };
    });

    // 子类可重写的生命周期钩子（基类均为空实现）
    init(): void | Promise<void>;
    getName(): string | undefined;
    getLatestVersion(): string | undefined;
    getChangelog(latestVersion: string): string | undefined;
    getIncrement(): string | undefined;
    getIncrementedVersionCI(options: Record<string, unknown>): string | undefined;
    getIncrementedVersion(options: Record<string, unknown>): string | undefined;
    beforeBump(): void | Promise<void>;
    bump(version: string): void | false | Promise<void | false>;
    beforeRelease(): void | Promise<void>;
    release(): void | Promise<void>;
    afterRelease(): void | Promise<void>;

    // 工具方法
    getInitialOptions(options: Record<string, unknown>, namespace: string): Record<string, unknown>;
    getContext(): Record<string, unknown>;
    getContext(path: string): unknown;
    setContext(context: Record<string, unknown>): void;
    exec(
      command: string,
      opts?: {
        options?: ShellExecOptions;
        context?: Record<string, unknown>;
      },
    ): Promise<unknown>;
    registerPrompts(prompts: Record<string, unknown>): void;
    showPrompt(options: Record<string, unknown>): Promise<unknown>;
    step(options: Record<string, unknown>): Promise<unknown>;
  }
}

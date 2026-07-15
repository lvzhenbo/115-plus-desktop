declare module 'release-it' {
  interface Log {
    exec(...args: unknown[]): void;
    verbose(...args: unknown[]): void;
    warn(...args: unknown[]): void;
    info(...args: unknown[]): void;
    error(...args: unknown[]): void;
    log(...args: unknown[]): void;
    obtrusive(...args: unknown[]): void;
    preview(options: { title: string; text: string }): void;
  }

  interface Config {
    readonly isDryRun: boolean;
    readonly isCI: boolean;
    readonly isIncrement: boolean;
    readonly isDebug: boolean;
    getContext(): Record<string, unknown>;
    getContext(path: string): unknown;
    setContext(options: Record<string, unknown>): void;
  }

  interface ShellExecOptions {
    write?: boolean;
    external?: boolean;
  }

  class Plugin {
    readonly namespace: string;
    readonly options: Record<string, unknown>;
    readonly config: Config;
    readonly log: Log;
    readonly debug: (...args: unknown[]) => void;
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
    }): Plugin;
    init(): void | Promise<void>;
    getName(): string | undefined;
    getLatestVersion(): string | undefined;
    getChangelog(latestVersion: string): string | Promise<string> | undefined;
    getIncrement(): string | undefined;
    getIncrementedVersionCI(options: Record<string, unknown>): string | undefined;
    getIncrementedVersion(options: Record<string, unknown>): string | undefined;
    beforeBump(): void | Promise<void>;
    bump(version: string): void | false | Promise<void | false>;
    beforeRelease(): void | Promise<void>;
    release(): void | Promise<void>;
    afterRelease(): void | Promise<void>;
    getInitialOptions(options: Record<string, unknown>, namespace: string): Record<string, unknown>;
    getContext(): Record<string, unknown>;
    getContext(path: string): unknown;
    setContext(context: Record<string, unknown>): void;
    exec(
      command: string,
      opts?: { options?: ShellExecOptions; context?: Record<string, unknown> },
    ): Promise<unknown>;
    registerPrompts(prompts: Record<string, unknown>): void;
    showPrompt(options: Record<string, unknown>): Promise<unknown>;
    step(options: Record<string, unknown>): Promise<unknown>;
  }
}

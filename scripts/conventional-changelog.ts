/**
 * 本地化 conventional-changelog 插件。
 * 直接使用 conventional-changelog-conventionalcommits v10 的 createPreset()，
 * 不通过名称字符串动态加载 preset，解决旧版本空 changelog bug。
 */
import { EOL } from 'node:os';
import fs from 'node:fs';
import { Plugin } from 'release-it';
import { Bumper } from 'conventional-recommended-bump';
import { ConventionalChangelog as ChangelogGenerator } from 'conventional-changelog';
import { ConventionalGitClient } from '@conventional-changelog/git-client';
import semver from 'semver';
import { text as streamText } from 'node:stream/consumers';
import createPreset from 'conventional-changelog-conventionalcommits';

// ============================================================================
// 基于源码定义的类型
// ============================================================================

/** conventional-changelog-conventionalcommits v10 返回的 preset 结构 */
interface PresetConfig extends Record<string, unknown> {
  commits: Record<string, unknown>;
  parser: Record<string, unknown>;
  writer: Record<string, unknown>;
  whatBump(commits: unknown[]): { level?: number; releaseType?: string };
}

/** conventional-recommended-bump v12 bumper 接口 */
interface BumperV12 {
  loadPreset(preset: unknown, loader?: (name: string) => Promise<unknown>): this;
  config(config: Record<string, unknown>): this;
  options(opts: Record<string, unknown>): this;
  tag(paramsOrTag: Record<string, unknown> | string): this;
  commits(params: Record<string, unknown>, parserOpts?: Record<string, unknown>): this;
  bump(whatBump: (commits: unknown[]) => { level?: number; releaseType?: string }): Promise<{
    level?: number;
    releaseType?: string;
    commits: unknown[];
  }>;
}

/** conventional-changelog v8 generator 接口 */
interface ChangelogGeneratorV8 {
  loadPreset(preset: Record<string, unknown>): void;
  options(opts: { releaseCount?: number; append?: boolean }): void;
  context(ctx: Record<string, unknown>): void;
  commits(gitRawCommitsOpts: Record<string, unknown>, parserOpts: Record<string, unknown>): void;
  writer(opts: Record<string, unknown>): void;
  readRepository(): void;
  writeStream(): NodeJS.ReadableStream;
}

/** @conventional-changelog/git-client v3 接口 */
interface GitClientV3 {
  getConfig(key: string): Promise<unknown>;
  getSemverTags(opts: { prefix?: string; skipUnstable?: boolean }): AsyncIterable<string>;
}

interface TypeConfig {
  type: string;
  section: string;
  hidden?: boolean;
}

interface UserPreset {
  types?: TypeConfig[];
  [key: string]: unknown;
}

/** 符合 Plugin 基类约束的选项类型 */
interface PluginOpts extends Record<string, unknown> {
  preset?: UserPreset;
  infile?: string;
  header?: string;
  tagPrefix?: string;
  tagOpts?: Record<string, unknown>;
  commitsOpts?: Record<string, unknown>;
  parserOpts?: Record<string, unknown>;
  writerOpts?: Record<string, unknown>;
  whatBump?: false | ((commits: unknown[]) => { level?: number; releaseType?: string });
  ignoreRecommendedBump?: boolean;
  strictSemVer?: boolean;
  cwd?: string;
}

// ============================================================================
// 类型断言仅在此处（外部包无官方类型）
// ============================================================================

const _Bumper = Bumper as unknown as new (cwd?: string) => BumperV12;
const _Generator = ChangelogGenerator as unknown as new (cwd: string) => ChangelogGeneratorV8;
const _GitClient = ConventionalGitClient as unknown as new (cwd: string) => GitClientV3;

// ============================================================================
// 构建 preset 配置
// ============================================================================

function buildPreset(userPreset?: UserPreset): PresetConfig {
  return createPreset(
    userPreset?.types ? { types: userPreset.types } : undefined,
  ) as unknown as PresetConfig;
}

// ============================================================================
// 插件主体
// ============================================================================

class ConventionalChangelog extends Plugin {
  declare readonly options: PluginOpts;
  declare debug: (...args: unknown[]) => void;

  static disablePlugin(opts: PluginOpts): 'version' | null {
    return opts.ignoreRecommendedBump ? null : 'version';
  }

  getTagPrefix(latestVersion: string): string {
    if (this.options.tagPrefix) return this.options.tagPrefix;
    const ctx = this.config.getContext();
    const latestTag = ctx.latestTag as string | undefined;
    return latestTag && latestVersion && latestTag.endsWith(latestVersion)
      ? latestTag.slice(0, latestTag.length - latestVersion.length)
      : '';
  }

  // ---- 版本推荐（使用 Bumper v12 API） ----

  async getRecommendedVersion(params: {
    increment?: string;
    latestVersion: string;
    isPreRelease?: boolean;
    preReleaseId?: string;
    preReleaseBase?: number;
  }): Promise<string | null> {
    const { increment, latestVersion, isPreRelease, preReleaseId, preReleaseBase } = params;
    const ctx = this.getContext();
    if (ctx.version) return ctx.version as string;

    const opts = this.options;
    const bumper = new _Bumper(opts.cwd);
    const preset = buildPreset(opts.preset);

    // v12: 用 config() 直接设置，不走 name 加载
    bumper.config(preset);
    if (opts.tagOpts) bumper.tag(opts.tagOpts);
    if (opts.commitsOpts || opts.parserOpts) {
      bumper.commits(opts.commitsOpts ?? {}, opts.parserOpts ?? {});
    }

    const whatBump =
      opts.whatBump === false
        ? () => ({ level: undefined, releaseType: undefined })
        : typeof opts.whatBump === 'function'
          ? opts.whatBump
          : preset.whatBump;

    const rec = await bumper.bump(whatBump);
    const releaseType: string | null =
      rec.releaseType ?? (rec.level !== undefined ? String(rec.level) : null);

    let finalType = releaseType;
    if (increment) {
      this.log.warn(
        `The recommended bump is "${releaseType}", but is overridden with "${increment}".`,
      );
      finalType = increment;
    }
    if (increment && semver.valid(increment)) return increment;

    if (isPreRelease) {
      if (finalType && (opts.strictSemVer || !semver.prerelease(latestVersion))) {
        return semverInc(
          latestVersion,
          `pre${finalType}` as semver.ReleaseType,
          preReleaseId,
          preReleaseBase,
        );
      }

      const gitClient = new _GitClient(opts.cwd ?? process.cwd());
      const tags: string[] = [];
      const tagPrefix = this.getTagPrefix(latestVersion);
      for await (const tag of gitClient.getSemverTags({ prefix: tagPrefix, skipUnstable: true })) {
        tags.push(tag);
      }

      const bump2 = await bumper.bump(whatBump);
      const toLast: string | null =
        bump2.releaseType ?? (bump2.level !== undefined ? String(bump2.level) : null);
      const lastStable = tags.length > 0 ? tags[0] : null;

      if (
        lastStable &&
        toLast &&
        (toLast === 'major' || toLast === 'minor' || toLast === 'patch')
      ) {
        const sm = semver as unknown as Record<string, (v: string) => string>;
        if (sm[toLast](lastStable) === sm[toLast](latestVersion)) {
          return semverInc(
            latestVersion,
            `pre${toLast}` as semver.ReleaseType,
            preReleaseId,
            preReleaseBase,
          );
        }
      }
      return semverInc(latestVersion, 'prerelease', preReleaseId, preReleaseBase);
    }

    if (finalType) {
      return semverInc(
        latestVersion,
        finalType as semver.ReleaseType,
        preReleaseId,
        preReleaseBase,
      );
    }
    return null;
  }

  // ---- Changelog 生成（使用 ChangelogGenerator v8 API） ----

  async getChangelog(latestVersion: string): Promise<string> {
    let ver = latestVersion || '0.0.0';
    if (!this.config.isIncrement) {
      this.setContext({ version: ver });
    } else {
      const vCtx = this.config.getContext('version') as {
        increment?: string;
        isPreRelease?: boolean;
        preReleaseId?: string;
        preReleaseBase?: number;
      };
      ver =
        (await this.getRecommendedVersion({
          increment: vCtx.increment,
          latestVersion: ver,
          isPreRelease: vCtx.isPreRelease,
          preReleaseId: vCtx.preReleaseId,
          preReleaseBase: vCtx.preReleaseBase,
        })) ?? ver;
      this.setContext({ version: ver });
    }
    return this.generateChangelog();
  }

  getChangelogStream(rawOptions: Record<string, unknown> = {}): Promise<NodeJS.ReadableStream> {
    const ctx = this.getContext();
    const version = ctx.version as string | undefined;
    const { isIncrement } = this.config;
    const gCtx = this.config.getContext();
    const latestTag = gCtx.latestTag as string | undefined;
    const secondLatestTag = gCtx.secondLatestTag as string | null | undefined;
    const tagTemplate = gCtx.tagTemplate as string | undefined;

    const currentTag = isIncrement
      ? tagTemplate
        ? tagTemplate.replace('${version}', version ?? '')
        : null
      : latestTag;
    const previousTag = isIncrement ? latestTag : secondLatestTag;
    const releaseCount = rawOptions.releaseCount === 0 ? 0 : isIncrement ? 1 : 2;

    const preset = buildPreset(this.options.preset);
    const gen = new _Generator(this.options.cwd ?? process.cwd());

    // v8: 不走 loadPreset（会尝试按 name 加载包），直接设 writer
    gen.writer(preset.writer);
    gen.options({ releaseCount });
    gen.context({ version, previousTag, currentTag });
    gen.commits({ from: previousTag }, preset.parser);
    gen.readRepository();

    return Promise.resolve(gen.writeStream());
  }

  generateChangelog(options?: Record<string, unknown>): Promise<string> {
    return this.getChangelogStream(options).then((s) => streamText(s).then((t) => t.trim()));
  }

  async getPreviousChangelog(): Promise<string> {
    return (await streamText(fs.createReadStream(String(this.options.infile)))).trim();
  }

  async writeChangelog(): Promise<void> {
    const { infile, header: rawHeader = '# Changelog' } = this.options;
    const ctx = this.config.getContext();
    let changelog: string | undefined = ctx.changelog as string | undefined;
    const header = String(rawHeader)
      .split(/\r\n|\r|\n/g)
      .join(EOL);

    let hasInfile = false;
    try {
      fs.accessSync(String(infile));
      hasInfile = true;
    } catch {
      /* 首次创建 */
    }

    let previous = '';
    if (hasInfile) {
      try {
        previous = (await this.getPreviousChangelog()).replace(header, '');
      } catch {
        /* 忽略 */
      }
    } else {
      changelog = await this.generateChangelog({ releaseCount: 0 });
    }

    fs.writeFileSync(
      String(infile),
      header +
        (changelog ? EOL + EOL + changelog.trim() : '') +
        (previous ? EOL + EOL + previous.trim() : '') +
        EOL,
    );
    if (!hasInfile) await this.exec(`git add ${infile}`);
  }

  // ---- release-it 生命周期 ----

  getIncrementedVersion(options: Record<string, unknown>): string | undefined {
    if (this.options.ignoreRecommendedBump) return undefined;
    return this.getRecommendedVersion({
      increment: options.increment as string | undefined,
      latestVersion: options.latestVersion as string,
      isPreRelease: options.isPreRelease as boolean | undefined,
      preReleaseId: options.preReleaseId as string | undefined,
      preReleaseBase: options.preReleaseBase as number | undefined,
    }) as unknown as string | undefined;
  }

  getIncrementedVersionCI(options: Record<string, unknown>): string | undefined {
    return this.getIncrementedVersion(options);
  }

  async bump(version: string): Promise<void> {
    const recommended = (this.getContext() as { version?: string }).version;
    this.setContext({ version });
    if (this.options.ignoreRecommendedBump && recommended !== version) {
      this.config.setContext({ changelog: await this.generateChangelog() });
    }
  }

  async beforeRelease(): Promise<void> {
    const { infile } = this.options;
    const { isDryRun } = this.config;
    this.log.exec(`Writing changelog to ${infile}`, isDryRun);
    if (infile && !isDryRun) await this.writeChangelog();
  }
}

// ============================================================================
// semver.inc 包装
// ============================================================================

function semverInc(
  version: string,
  release: semver.ReleaseType,
  identifier?: string,
  identifierBase?: number,
): string | null {
  if (identifier !== undefined) {
    return semver.inc(version, release, identifier, identifierBase as unknown as undefined);
  }
  return semver.inc(version, release);
}

export default ConventionalChangelog;

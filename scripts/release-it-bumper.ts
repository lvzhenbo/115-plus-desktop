/**
 * 自定义 release-it 插件，用于 Tauri 项目的版本发布流程：
 *   - bump:   将新版本号同步到 tauri.conf.json / Cargo.toml
 *   - beforeRelease: 校验版本一致性 + 更新 Cargo.lock
 *
 * 替代 @release-it/bumper，零外部依赖。
 *
 * 配置示例（.release-it.json）：
 *   "./scripts/release-it-bumper.ts": {
 *     "out": [
 *       { "file": "src-tauri/tauri.conf.json", "path": "version" },
 *       "src-tauri/Cargo.toml"
 *     ]
 *   }
 *
 * out 数组每项支持：
 *   - 字符串：按扩展名推断类型（.json → JSON 根级 version 字段，.toml → [package] version）
 *   - 对象：{ file, path } 显式指定 JSON 中要更新的字段路径
 */
import { Plugin } from 'release-it';
import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

// ---------------------------------------------------------------------------
// 类型定义
// ---------------------------------------------------------------------------

interface OutEntryObject {
  file: string;
  path?: string;
}

type OutEntry = string | OutEntryObject;

interface BumperOptions {
  out?: OutEntry[];
}

/** 需要校验版本一致性的文件列表 */
const VERSION_FILES = [
  { file: 'package.json', type: 'json' as const, key: 'version' },
  { file: 'src-tauri/tauri.conf.json', type: 'json' as const, key: 'version' },
  { file: 'src-tauri/Cargo.toml', type: 'toml' as const },
] as const;

// ---------------------------------------------------------------------------
// 插件主体
// ---------------------------------------------------------------------------

export default class BumperPlugin extends Plugin {
  /**
   * 将新版本号写入所有配置的 out 文件
   */
  async bump(newVersion: string): Promise<void | false> {
    const { out } = this.options as BumperOptions;
    if (!out) return false;

    const isDryRun: boolean = this.config.isDryRun;
    const entries = Array.isArray(out) ? out : [out];

    for (const entry of entries) {
      if (typeof entry === 'string') {
        await this.writeFile(entry, 'version', newVersion, isDryRun);
      } else {
        await this.writeFile(entry.file, entry.path || 'version', newVersion, isDryRun);
      }
    }
  }

  /**
   * bump 之后、git commit 之前：
   *   1) 校验所有版本文件一致
   *   2) 重新生成 Cargo.lock
   */
  async beforeRelease(): Promise<void> {
    const version = this.config.getContext().version as string;
    if (!version || this.config.isDryRun) return;

    await this.validateVersions(version);

    this.log.exec('cargo generate-lockfile --manifest-path src-tauri/Cargo.toml');
    await this.exec('cargo generate-lockfile --manifest-path src-tauri/Cargo.toml');
    this.log.verbose('  Cargo.lock 已更新');
  }

  // -----------------------------------------------------------------------
  // 内部方法
  // -----------------------------------------------------------------------

  /**
   * 根据文件扩展名推断类型并写入版本号
   */
  private async writeFile(
    file: string,
    key: string,
    version: string,
    isDryRun: boolean,
  ): Promise<void> {
    const absolutePath = path.resolve(file);
    const ext = path.extname(file).toLowerCase();

    this.log.exec(`写入版本号到 ${file}`, isDryRun);
    if (isDryRun) return;

    try {
      if (ext === '.json') {
        await this.writeJson(absolutePath, key, version);
      } else if (ext === '.toml') {
        await this.writeToml(absolutePath, version);
      } else {
        this.log.warn(`不支持的文件类型: ${file}`);
      }
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      this.log.warn(`写入 ${file} 失败: ${message}`);
    }
  }

  /**
   * 写入 JSON 文件（保留 2 空格缩进）
   */
  private async writeJson(filePath: string, key: string, version: string): Promise<void> {
    const content = await readFile(filePath, 'utf-8');
    const json: Record<string, unknown> = JSON.parse(content);
    json[key] = version;
    await writeFile(filePath, JSON.stringify(json, null, 2) + '\n');
    this.log.verbose(`  ${key}: ${version}`);
  }

  /**
   * 写入 Cargo.toml（更新 [package] 下的 version 字段）
   */
  private async writeToml(filePath: string, version: string): Promise<void> {
    const content = await readFile(filePath, 'utf-8');
    // 仅匹配第一个 version = "..."（即 [package] 段下的）
    const newContent = content.replace(/^version\s*=\s*"[^"]*"/m, `version = "${version}"`);
    if (newContent === content) {
      this.log.warn(`  在 ${filePath} 中未找到 version 字段`);
      return;
    }
    await writeFile(filePath, newContent);
    this.log.verbose(`  version: ${version}`);
  }

  /**
   * 校验各版本文件中的版本号是否一致
   */
  private async validateVersions(expected: string): Promise<void> {
    let allMatch = true;

    for (const item of VERSION_FILES) {
      try {
        const content = await readFile(item.file, 'utf-8');
        let actual: string | undefined;

        if (item.type === 'json') {
          actual = (JSON.parse(content) as Record<string, unknown>)[item.key] as string | undefined;
        } else {
          actual = content.match(/^version\s*=\s*"([^"]*)"/m)?.[1];
        }

        if (actual && actual !== expected) {
          this.log.warn(`⚠ 版本不一致: ${item.file} (${actual}) ≠ ${expected}`);
          allMatch = false;
        }
      } catch {
        this.log.warn(`⚠ 无法读取 ${item.file}，跳过校验`);
      }
    }

    if (allMatch) {
      this.log.verbose('✅ 所有版本文件一致');
    }
  }
}

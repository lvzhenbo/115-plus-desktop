import fs from 'fs';
import { fileURLToPath } from 'url';
import path from 'path';
import { ZipReader, BlobReader, BlobWriter } from '@zip.js/zip.js';
import { mkdir } from 'fs/promises';
import { ProxyAgent, fetch as undiciFetch, type RequestInit } from 'undici';

// 从环境变量读取代理配置，或自己修改为你需要的代理地址
const proxyUrl =
  process.env.HTTPS_PROXY ||
  process.env.https_proxy ||
  process.env.HTTP_PROXY ||
  process.env.http_proxy;
const dispatcher = proxyUrl ? new ProxyAgent(proxyUrl) : undefined;
if (proxyUrl) {
  console.log(`🌐 使用代理: ${proxyUrl}`);
}

// 带重试的 fetch，处理 GitHub API 速率限制 (403/429)
async function fetchWithRetry(
  url: string,
  options?: RequestInit,
  maxRetries = 3,
): Promise<import('undici').Response> {
  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    const response = await undiciFetch(url, { dispatcher, ...options });

    if (response.status !== 403 && response.status !== 429) {
      return response;
    }

    // 非速率限制的 403 直接抛出
    const rateLimitRemaining = response.headers.get('x-ratelimit-remaining');
    if (response.status === 403 && rateLimitRemaining !== '0') {
      return response;
    }

    if (attempt === maxRetries) {
      return response;
    }

    // 计算等待时间
    let waitSeconds: number;
    const retryAfter = response.headers.get('retry-after');
    const rateLimitReset = response.headers.get('x-ratelimit-reset');

    if (retryAfter) {
      waitSeconds = parseInt(retryAfter, 10) || 60;
    } else if (rateLimitReset) {
      waitSeconds = Math.max(0, parseInt(rateLimitReset, 10) - Math.floor(Date.now() / 1000)) + 1;
    } else {
      waitSeconds = 60;
    }

    console.log(`⏳ GitHub API 速率限制，${waitSeconds}秒后重试 (${attempt + 1}/${maxRetries})...`);
    await new Promise((resolve) => setTimeout(resolve, waitSeconds * 1000));
  }

  throw new Error('重试次数已耗尽');
}

// 定义GitHub API响应的类型
interface GitHubAsset {
  name: string;
  browser_download_url: string;
}

interface GitHubRelease {
  tag_name: string;
  assets: GitHubAsset[];
}

// 使用ES模块获取__dirname等价物
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// 使用zip.js解压ZIP文件
const extractZip = async (source: string, options: { dir: string }) => {
  const { dir: targetDir } = options;

  try {
    // 读取ZIP文件
    const zipFileData = fs.readFileSync(source);
    const zipReader = new ZipReader(new BlobReader(new Blob([zipFileData])));

    // 获取所有条目
    const entries = await zipReader.getEntries();

    for (const entry of entries) {
      const fileName = entry.filename;

      // 如果是目录，则创建目录
      if (entry.directory) {
        await mkdir(path.join(targetDir, fileName), { recursive: true });
        continue;
      }

      // 创建输出目录
      const outputPath = path.join(targetDir, fileName);
      await mkdir(path.dirname(outputPath), { recursive: true });

      // 读取条目内容并写入文件
      if (entry.getData) {
        const blob = await entry.getData(new BlobWriter());
        const arrayBuffer = await blob.arrayBuffer();
        const buffer = Buffer.from(arrayBuffer);

        fs.writeFileSync(outputPath, buffer);
      }
    }

    // 关闭ZIP读取器
    await zipReader.close();
  } catch (error) {
    throw new Error(`解压ZIP文件失败: ${error instanceof Error ? error.message : String(error)}`);
  }
};

// 目标目录
const binariesDir = path.join(__dirname, '../src-tauri/binaries');

// 创建目标目录(如果不存在)
if (!fs.existsSync(binariesDir)) {
  fs.mkdirSync(binariesDir, { recursive: true });
}

// Rust目标平台映射表
const platformMap = {
  'windows-win64': 'x86_64-pc-windows-msvc', // Windows 64位
};

// GitHub API URL，获取最新版本
const githubApiUrl = 'https://api.github.com/repos/aria2/aria2/releases/latest';

async function downloadLatestAria2() {
  try {
    console.log('📥 获取aria2最新版本信息...');
    const response = await fetchWithRetry(githubApiUrl);
    if (!response.ok) {
      throw new Error(`GitHub API 请求失败: ${response.status} ${response.statusText}`);
    }
    const data = (await response.json()) as GitHubRelease;
    const latestVersion = data.tag_name;
    console.log(`🔍 找到最新版本: ${latestVersion}`);

    const assets = data.assets;

    // 筛选Windows 64位平台的资源
    let win64Asset: GitHubAsset | null = null;

    for (const asset of assets) {
      if (
        (asset.name.includes('win64') || asset.name.includes('win-64')) &&
        asset.name.endsWith('.zip')
      ) {
        win64Asset = asset;
        break; // 找到合适的Windows 64位ZIP文件后立即退出循环
      }
    }

    if (!win64Asset) {
      throw new Error('没有找到Windows 64位平台的aria2下载文件');
    }

    // 处理Windows 64位平台文件
    await processAsset(win64Asset, 'windows-win64', platformMap['windows-win64']);

    console.log('✅ aria2二进制文件下载和处理完成!');
  } catch (error) {
    console.error('❌ 下载过程中发生错误:', error instanceof Error ? error.message : String(error));
    throw error;
  }
}

async function processAsset(asset: GitHubAsset, platform: string, rustTriple: string) {
  try {
    console.log(`📦 处理Windows 64位平台: ${platform} (${rustTriple})`);

    // 下载文件
    console.log(`🔗 下载: ${asset.name} 从 ${asset.browser_download_url}`);
    const downloadResponse = await fetchWithRetry(asset.browser_download_url, {
      signal: AbortSignal.timeout(1000 * 60 * 1),
    });
    if (!downloadResponse.ok) {
      throw new Error(`下载失败: ${downloadResponse.status} ${downloadResponse.statusText}`);
    }

    const tempDir = path.join(__dirname, 'temp');
    if (!fs.existsSync(tempDir)) {
      fs.mkdirSync(tempDir, { recursive: true });
    }

    // 保存文件
    const downloadPath = path.join(tempDir, asset.name);
    const arrayBuffer = await downloadResponse.arrayBuffer();
    fs.writeFileSync(downloadPath, Buffer.from(arrayBuffer));
    console.log(`✅ 下载完成: ${downloadPath}`);

    // 提取文件
    const extractDir = path.join(tempDir, `extract_windows`);
    if (fs.existsSync(extractDir)) {
      fs.rmSync(extractDir, { recursive: true, force: true });
    }
    fs.mkdirSync(extractDir, { recursive: true });

    console.log(`📂 解压ZIP: ${asset.name}`);
    await extractZip(downloadPath, { dir: extractDir });

    // 查找aria2c可执行文件
    let aria2Binary: string | null = null;

    function findAria2Binary(dir: string): string | null {
      const files = fs.readdirSync(dir, { withFileTypes: true });

      for (const file of files) {
        const fullPath = path.join(dir, file.name);

        if (file.isDirectory()) {
          const result = findAria2Binary(fullPath);
          if (result) return result;
        } else if (file.name === 'aria2c.exe' || file.name.startsWith('aria2c-')) {
          return fullPath;
        }
      }

      return null;
    }

    aria2Binary = findAria2Binary(extractDir);

    if (!aria2Binary) {
      console.error(`❌ 未找到 aria2c.exe 可执行文件`);
      return;
    }

    console.log(`🔍 找到aria2二进制文件: ${aria2Binary}`);

    // 目标文件名
    const targetFileName = `aria2c-${rustTriple}.exe`;

    // 拷贝到目标目录
    const targetPath = path.join(binariesDir, targetFileName);
    fs.copyFileSync(aria2Binary, targetPath);
    console.log(`✅ 已拷贝到: ${targetPath}`);

    // 清理临时文件
    fs.unlinkSync(downloadPath);
    fs.rmSync(extractDir, { recursive: true, force: true });
    console.log(`🧹 已清理临时文件`);
  } catch (error) {
    console.error(
      `❌ 处理 ${asset.name} 时出错:`,
      error instanceof Error ? error.message : String(error),
    );
    throw error;
  }
}

// 清理临时目录
function cleanup() {
  const tempDir = path.join(__dirname, 'temp');
  if (fs.existsSync(tempDir)) {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
}

// 启动下载
downloadLatestAria2()
  .then(() => {
    cleanup();
    console.log('🎉 任务完成!');
  })
  .catch((err) => {
    console.error('❌ 出错:', err);
    process.exit(1);
  });

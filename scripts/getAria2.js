import axios from 'axios';
import fs from 'fs';
import { fileURLToPath } from 'url';
import path from 'path';
import { Buffer } from 'buffer';
import * as console from 'console';
import process from 'process';
import yauzl from 'yauzl';
import { createWriteStream } from 'fs';
import { mkdir } from 'fs/promises';

// 使用ES模块获取__dirname等价物
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// 使用yauzl解压ZIP文件
const extractZip = async (source, options) => {
  const { dir: targetDir } = options;

  return new Promise((resolve, reject) => {
    yauzl.open(source, { lazyEntries: true }, (err, zipfile) => {
      if (err) {
        return reject(new Error(`打开ZIP文件失败: ${err.message}`));
      }

      zipfile.on('entry', async (entry) => {
        try {
          // 获取文件路径，处理目录项
          const fileName = entry.fileName;

          // 如果是目录，则创建目录并继续下一条目
          if (/\/$/.test(fileName)) {
            await mkdir(path.join(targetDir, fileName), { recursive: true });
            zipfile.readEntry(); // 继续读取下一个条目
            return;
          }

          // 创建输出目录
          const outputPath = path.join(targetDir, fileName);
          await mkdir(path.dirname(outputPath), { recursive: true });

          // 读取该条目内容
          zipfile.openReadStream(entry, async (err, readStream) => {
            if (err) {
              console.error(`读取ZIP条目失败: ${err.message}`);
              zipfile.readEntry();
              return;
            }

            // 创建输出流
            const writeStream = createWriteStream(outputPath);

            // 完成写入后处理
            writeStream.on('close', () => {
              zipfile.readEntry(); // 处理下一个条目
            });

            // 错误处理
            readStream.on('error', (err) => {
              console.error(`解压条目时出错: ${err.message}`);
              zipfile.readEntry();
            });

            // 管道连接，将内容写入文件
            readStream.pipe(writeStream);
          });
        } catch (err) {
          console.error(`处理ZIP条目失败: ${err.message}`);
          zipfile.readEntry(); // 出错时也继续处理下一个条目
        }
      });

      // 处理解压缩结束
      zipfile.on('end', () => {
        resolve();
      });

      // 处理错误
      zipfile.on('error', (err) => {
        reject(new Error(`解压缩过程中发生错误: ${err.message}`));
      });

      // 开始读取ZIP条目
      zipfile.readEntry();
    });
  });
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
    const response = await axios.get(githubApiUrl);
    const latestVersion = response.data.tag_name;
    console.log(`🔍 找到最新版本: ${latestVersion}`);

    const assets = response.data.assets;

    // 筛选Windows 64位平台的资源
    let win64Asset = null;

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
    console.error('❌ 下载过程中发生错误:', error.message);
    throw error;
  }
}

async function processAsset(asset, platform, rustTriple) {
  try {
    console.log(`📦 处理Windows 64位平台: ${platform} (${rustTriple})`);

    // 下载文件
    console.log(`🔗 下载: ${asset.name} 从 ${asset.browser_download_url}`);
    const downloadResponse = await axios({
      method: 'get',
      url: asset.browser_download_url,
      responseType: 'arraybuffer',
      timeout: 1000 * 60 * 1,
    });

    const tempDir = path.join(__dirname, 'temp');
    if (!fs.existsSync(tempDir)) {
      fs.mkdirSync(tempDir, { recursive: true });
    }

    // 保存文件
    const downloadPath = path.join(tempDir, asset.name);
    fs.writeFileSync(downloadPath, Buffer.from(downloadResponse.data));
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
    let aria2Binary = null;

    function findAria2Binary(dir) {
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
    console.error(`❌ 处理 ${asset.name} 时出错:`, error.message);
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

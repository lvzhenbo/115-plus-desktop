import { useSettingStore } from '@/store/setting';
import { useUserStore } from '@/store/user';
import { aria2Server } from '@/utils/http/alova';
import type { Aria2Response, Aria2Task, Aria2GlobalStat } from './types/aria2';

const settingStore = useSettingStore();
const userStore = useUserStore();

let idCounter = 0;
const nextId = () => `aria2-${++idCounter}`;

export const getVersion = () =>
  aria2Server.Post('/jsonrpc', {
    jsonrpc: '2.0',
    id: nextId(),
    method: 'aria2.getVersion',
  });

export const addUri = (url: string, name: string, path?: string) =>
  aria2Server.Post<Aria2Response<string>>('/jsonrpc', {
    jsonrpc: '2.0',
    id: nextId(),
    method: 'aria2.addUri',
    params: [
      [url],
      {
        dir: settingStore.downloadSetting.downloadPath + (path ? `/${path}` : ''),
        out: name,
        'max-connection-per-server': '16',
        split: '16',
        'min-split-size': '1M',
        header: [
          `User-Agent: ${navigator.userAgent}`,
          `Authorization: Bearer ${userStore.accessToken}`,
        ],
      },
    ],
  });

export const tellStatus = (gid: string) =>
  aria2Server.Post<Aria2Response<Aria2Task>>('/jsonrpc', {
    jsonrpc: '2.0',
    id: nextId(),
    method: 'aria2.tellStatus',
    params: [
      gid,
      [
        'gid',
        'status',
        'totalLength',
        'completedLength',
        'downloadSpeed',
        'files',
        'errorCode',
        'errorMessage',
      ],
    ],
  });

/**
 * 批量查询下载状态（使用 system.multicall 减少请求次数）
 */
export const batchTellStatus = (gids: string[]) =>
  aria2Server.Post<Aria2Response<Aria2Response<Aria2Task>[]>>('/jsonrpc', {
    jsonrpc: '2.0',
    id: nextId(),
    method: 'system.multicall',
    params: [
      gids.map((gid) => ({
        methodName: 'aria2.tellStatus',
        params: [
          gid,
          [
            'gid',
            'status',
            'totalLength',
            'completedLength',
            'downloadSpeed',
            'files',
            'errorCode',
            'errorMessage',
          ],
        ],
      })),
    ],
  });

/**
 * 获取正在下载的任务列表
 */
export const tellActive = () =>
  aria2Server.Post<Aria2Response<Aria2Task[]>>('/jsonrpc', {
    jsonrpc: '2.0',
    id: nextId(),
    method: 'aria2.tellActive',
    params: [['gid', 'status', 'totalLength', 'completedLength', 'downloadSpeed', 'files']],
  });

/**
 * 获取等待中的任务列表
 */
export const tellWaiting = (offset: number = 0, num: number = 100) =>
  aria2Server.Post<Aria2Response<Aria2Task[]>>('/jsonrpc', {
    jsonrpc: '2.0',
    id: nextId(),
    method: 'aria2.tellWaiting',
    params: [
      offset,
      num,
      ['gid', 'status', 'totalLength', 'completedLength', 'downloadSpeed', 'files'],
    ],
  });

/**
 * 获取已停止的任务列表
 */
export const tellStopped = (offset: number = 0, num: number = 100) =>
  aria2Server.Post<Aria2Response<Aria2Task[]>>('/jsonrpc', {
    jsonrpc: '2.0',
    id: nextId(),
    method: 'aria2.tellStopped',
    params: [
      offset,
      num,
      ['gid', 'status', 'totalLength', 'completedLength', 'downloadSpeed', 'files'],
    ],
  });

/**
 * 获取全局下载统计
 */
export const getGlobalStat = () =>
  aria2Server.Post<Aria2Response<Aria2GlobalStat>>('/jsonrpc', {
    jsonrpc: '2.0',
    id: nextId(),
    method: 'aria2.getGlobalStat',
  });

export const remove = (gid: string) =>
  aria2Server.Post('/jsonrpc', {
    jsonrpc: '2.0',
    id: nextId(),
    method: 'aria2.remove',
    params: [gid],
  });

export const forceRemove = (gid: string) =>
  aria2Server.Post('/jsonrpc', {
    jsonrpc: '2.0',
    id: nextId(),
    method: 'aria2.forceRemove',
    params: [gid],
  });

export const removeDownloadResult = (gid: string) =>
  aria2Server.Post('/jsonrpc', {
    jsonrpc: '2.0',
    id: nextId(),
    method: 'aria2.removeDownloadResult',
    params: [gid],
  });

export const purgeDownloadResult = () =>
  aria2Server.Post('/jsonrpc', {
    jsonrpc: '2.0',
    id: nextId(),
    method: 'aria2.purgeDownloadResult',
  });

export const pause = (gid: string) =>
  aria2Server.Post('/jsonrpc', {
    jsonrpc: '2.0',
    id: nextId(),
    method: 'aria2.pause',
    params: [gid],
  });

export const unpause = (gid: string) =>
  aria2Server.Post('/jsonrpc', {
    jsonrpc: '2.0',
    id: nextId(),
    method: 'aria2.unpause',
    params: [gid],
  });

export const pauseAll = () =>
  aria2Server.Post('/jsonrpc', {
    jsonrpc: '2.0',
    id: nextId(),
    method: 'aria2.pauseAll',
  });

export const unpauseAll = () =>
  aria2Server.Post('/jsonrpc', {
    jsonrpc: '2.0',
    id: nextId(),
    method: 'aria2.unpauseAll',
  });

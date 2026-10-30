import { alovaInst, type ResponseData } from '@/utils/http/alova';
import { openBaseUrl } from './config';
import type {
  QuotaInfoResponseData,
  TaskListResponseData,
  TorrentInfo,
  TorrentTaskParams,
} from './types/cloud';

export const taskList = (params: { page: number }) =>
  alovaInst.Get<ResponseData<TaskListResponseData>>(`${openBaseUrl}/open/offline/get_task_list`, {
    params,
    cacheFor: null,
  });

export const torrentParse = (data: { torrent_sha1: string; pick_code: string }) =>
  alovaInst.Post<ResponseData<TorrentInfo>>(`${openBaseUrl}/open/offline/torrent`, data, {
    cacheFor: null,
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
    },
  });

export const torrentTaskAdd = (data: TorrentTaskParams) =>
  alovaInst.Post<ResponseData<unknown>>(`${openBaseUrl}/open/offline/add_task_bt`, data, {
    cacheFor: null,
    name: 'urlTaskAdd',
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
    },
  });

export const taskDelete = (data: { info_hash: string; del_source_file?: 1 | 0 }) =>
  alovaInst.Post<ResponseData<unknown>>(`${openBaseUrl}/open/offline/del_task`, data, {
    cacheFor: null,
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
    },
  });

export const quotaInfo = () =>
  alovaInst.Get<ResponseData<QuotaInfoResponseData>>(`${openBaseUrl}/open/offline/get_quota_info`, {
    hitSource: 'urlTaskAdd',
  });

export const urlTaskAdd = (data: { urls: string; wp_path_id?: string }) =>
  alovaInst.Post<ResponseData<unknown>>(`${openBaseUrl}/open/offline/add_task_urls`, data, {
    cacheFor: null,
    name: 'urlTaskAdd',
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
    },
  });

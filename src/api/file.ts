import { alovaInst, type ResponseData } from '@/utils/http/alova';
import { openBaseUrl } from './config';
import type {
  FileDeatil,
  FileDownloadUrlResponseData,
  FileListRequestParams,
  FileListResponseData,
  RecycleBinListResponseData,
} from './types/file';

/**
 * 获取文件列表
 */
export const fileList = (params: FileListRequestParams) =>
  alovaInst.Get<FileListResponseData>(`${openBaseUrl}/open/ufile/files`, {
    params,
    cacheFor: null,
  });

/**
 * 获取文件详情
 */
export const fileDetail = (params: { file_id: string }) =>
  alovaInst.Get<ResponseData<FileDeatil>>(`${openBaseUrl}/open/folder/get_info`, {
    params,
  });

/**
 * 新建文件夹
 */
export const addFolder = (data: { file_name: string; pid: string }) =>
  alovaInst.Post<ResponseData<{ file_name: string; file_id: string }>>(
    `${openBaseUrl}/open/folder/add`,
    data,
    {
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
      },
    },
  );

/**
 * 复制文件
 */
export const copyFile = (data: { file_id: string; pid: string; nodupli?: '0' | '1' }) =>
  alovaInst.Post<ResponseData<unknown>>(`${openBaseUrl}/open/ufile/copy`, data, {
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
    },
  });

/**
 * 移动文件
 */
export const moveFile = (data: { file_ids: string; to_cid: string }) =>
  alovaInst.Post<ResponseData<unknown>>(`${openBaseUrl}/open/ufile/move`, data, {
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
    },
  });

/**
 * 重命名文件
 */
export const updateFile = (data: { file_id: string; file_name: string }) =>
  alovaInst.Post<ResponseData<unknown>>(`${openBaseUrl}/open/ufile/update`, data, {
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
    },
  });

/**
 * 删除文件
 */
export const deleteFile = (data: { file_ids: string }) =>
  alovaInst.Post<ResponseData<unknown>>(`${openBaseUrl}/open/ufile/delete`, data, {
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
    },
  });

/**
 * 回收站列表
 */
export const recycleBinList = (params: { limit: number; offset: number }) =>
  alovaInst.Get<ResponseData<RecycleBinListResponseData>>(`${openBaseUrl}/open/rb/list`, {
    params,
    cacheFor: null,
  });

/**
 * 恢复文件
 */
export const revertFile = (data: { tid: string }) =>
  alovaInst.Post<ResponseData<unknown>>(`${openBaseUrl}/open/rb/revert`, data, {
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
    },
  });

/**
 * 删除清空回收站
 */
export const deleteRecycleBinFile = (data?: { tid?: string }) =>
  alovaInst.Post<ResponseData<unknown>>(`${openBaseUrl}/open/rb/del`, data, {
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
    },
  });

/**
 * 获取文件下载地址
 */
export const fileDownloadUrl = (data: { pick_code: string }) =>
  alovaInst.Post<ResponseData<FileDownloadUrlResponseData>>(
    `${openBaseUrl}/open/ufile/downurl`,
    data,
    {
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
      },
    },
  );

import { alovaInst, type ResponseData } from '@/utils/http/alova';
import { openBaseUrl } from './config';
import type { FileDeatil, FileListRequestParams, FileListResponseData } from './types/file';

export const fileList = (params: FileListRequestParams) =>
  alovaInst.Get<FileListResponseData>(`${openBaseUrl}/open/ufile/files`, {
    params,
    cacheFor: null,
  });

export const fileDetail = (params: { file_id: string }) =>
  alovaInst.Get<ResponseData<FileDeatil>>(`${openBaseUrl}/open/folder/get_info`, {
    params,
  });

export const addFolder = (data: { file_name: string; pid: string }) =>
  alovaInst.Post<ResponseData<{ file_name: string; file_id: string }>>(
    `${openBaseUrl}/open/folder/add`,
    data,
    {
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
      },
      cacheFor: null,
    },
  );

export const copyFile = (data: { file_id: string; pid: string; nodupli?: '0' | '1' }) =>
  alovaInst.Post<ResponseData<unknown>>(`${openBaseUrl}/open/ufile/copy`, data, {
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
    },
    cacheFor: null,
  });

export const moveFile = (data: { file_ids: string; to_cid: string }) =>
  alovaInst.Post<ResponseData<unknown>>(`${openBaseUrl}/open/ufile/move`, data, {
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
    },
    cacheFor: null,
  });

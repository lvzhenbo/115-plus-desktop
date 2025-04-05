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

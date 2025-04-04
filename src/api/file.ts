import { alovaInst } from '@/utils/http/alova';
import { openBaseUrl } from './config';
import type { FileListRequestParams, FileListResponseData } from './types/file';

export const fileList = (params: FileListRequestParams) =>
  alovaInst.Get<FileListResponseData>(`${openBaseUrl}/open/ufile/files`, {
    params,
    cacheFor: null,
  });

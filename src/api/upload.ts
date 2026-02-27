import { alovaInst, type ResponseData } from '@/utils/http/alova';
import { openBaseUrl } from './config';
import type {
  UploadInitParams,
  UploadInitDataItem,
  UploadTokenData,
  UploadResumeParams,
  UploadResumeDataItem,
} from './types/upload';

/**
 * 文件上传初始化
 * POST /open/upload/init
 */
export const uploadInit = (data: UploadInitParams) =>
  alovaInst.Post<ResponseData<UploadInitDataItem>>(`${openBaseUrl}/open/upload/init`, data, {
    cacheFor: null,
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
    },
  });

/**
 * 获取上传凭证
 * GET /open/upload/get_token
 */
export const uploadGetToken = () =>
  alovaInst.Get<ResponseData<UploadTokenData>>(`${openBaseUrl}/open/upload/get_token`, {
    cacheFor: null,
  });

/**
 * 断点续传
 * POST /open/upload/resume
 */
export const uploadResume = (data: UploadResumeParams) =>
  alovaInst.Post<ResponseData<UploadResumeDataItem>>(`${openBaseUrl}/open/upload/resume`, data, {
    cacheFor: null,
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
    },
  });

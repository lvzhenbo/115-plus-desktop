import { alovaInst, type ResponseData } from '@/utils/http/alova';
import { openBaseUrl } from './config';
import type { VideoPlayUrlResponseData } from './types/video';

/**
 * 获取视频在线播放地址
 */
export const videoPlayUrl = (params: { pick_code: string }) =>
  alovaInst.Get<ResponseData<VideoPlayUrlResponseData>>(`${openBaseUrl}/open/video/play`, {
    params,
    cacheFor: null,
  });

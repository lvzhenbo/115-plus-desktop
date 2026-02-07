import { alovaInst, type ResponseData } from '@/utils/http/alova';
import { openBaseUrl } from './config';
import type {
  VideoHistoryResponseData,
  VideoPlayUrlResponseData,
  VideoSubtitleResponseData,
} from './types/video';

/**
 * 获取视频在线播放地址
 */
export const videoPlayUrl = (params: { pick_code: string }) =>
  alovaInst.Get<ResponseData<VideoPlayUrlResponseData>>(`${openBaseUrl}/open/video/play`, {
    params,
    cacheFor: null,
  });

export const videoHistory = (params: { pick_code: string }) =>
  alovaInst.Get<ResponseData<VideoHistoryResponseData>>(`${openBaseUrl}/open/video/history`, {
    params,
    cacheFor: null,
  });

export const saveVideoHistory = (data: { pick_code: string; time?: number; watch_end?: 0 | 1 }) =>
  alovaInst.Post<ResponseData<unknown>>(`${openBaseUrl}/open/video/history`, data, {
    cacheFor: null,
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
    },
  });

/**
 * 获取视频字幕列表
 */
export const videoSubtitle = (params: { pick_code: string }) =>
  alovaInst.Get<ResponseData<VideoSubtitleResponseData>>(`${openBaseUrl}/open/video/subtitle`, {
    params,
    cacheFor: null,
  });

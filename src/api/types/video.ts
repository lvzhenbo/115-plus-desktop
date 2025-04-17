export interface VideoPlayUrlResponseData {
  file_id: string;
  parent_id: string;
  file_name: string;
  file_size: string;
  file_sha1: string;
  file_type: string;
  is_private: string;
  play_long: string;
  user_def: number;
  user_rotate: number;
  user_turn: number;
  multitrack_list: any[];
  /**
   * @deprecated
   * 视频清晰度列表，使用 `definition_list_new` 替代
   */
  definition_list: { [key: string]: string };
  /**
   * 视频所有用可切换的清晰度列表;1:标清 2:高清 3:超清 4:1080P 5:4k;100:原画
   */
  definition_list_new: { [key: string]: string };
  /**
   * 视频各清晰度的播放地址信息
   */
  video_url: VideoURL[];
}

export interface VideoURL {
  url: string;
  height: number | string;
  width: number | string;
  /**
   * @deprecated
   * 视频清晰度，使用 `definition_n` 替代
   */
  definition: number;
  /**
   * 视频清晰度名称
   */
  title: string;
  /**
   * 视频清晰度(新)
   */
  definition_n: number;
}

export interface VideoHistoryResponseData {
  add_time: number;
  file_id: string;
  file_name: string;
  hash: string;
  pick_code: string;
  time: number;
}

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
  definition_list: { [key: string]: string };
  definition_list_new: { [key: string]: string };
  video_url: VideoURL[];
}

export interface VideoURL {
  url: string;
  height: number | string;
  width: number | string;
  definition: number;
  title: string;
  definition_n: number;
}

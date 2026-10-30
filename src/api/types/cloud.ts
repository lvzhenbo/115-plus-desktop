export interface TorrentInfo {
  file_size: number;
  torrent_name: string;
  file_count: number;
  info_hash: string;
  torrent_filelist: {
    size: number;
    path: string;
    wanted: number;
  }[];
}

export interface TorrentTaskParams {
  info_hash: string;
  /** 解析结果中的原始文件索引，以半角逗号分隔。 */
  wanted: string;
  save_path: string;
  torrent_sha1: string;
  pick_code: string;
  wp_path_id?: string;
}

export interface TaskListResponseData {
  page: number;
  page_count: number;
  count: number;
  tasks: Task[];
}

export interface Task {
  /**
   * 任务sha1
   */
  info_hash: string;
  /**
   * 任务添加时间戳
   */
  add_time: number;
  /**
   * 任务下载进度
   */
  percentDone: number;
  /**
   * 任务总大小（字节）
   */
  size: number;
  peers: number;
  rateDownload: number;
  /**
   * 任务名
   */
  name: string;
  /**
   * 任务最后更新时间戳
   */
  last_update: number;
  left_time: number;
  /**
   * 任务源文件（夹）对应文件（夹）id
   */
  file_id: string;
  /** 文件／目录类型：0 文件夹，1 文件；部分任务尚未生成源文件时不返回。 */
  file_category?: 0 | 1;
  /** 源文件图标类型。 */
  file_icon?: string;
  /**
   * 删除任务需删除源文件（夹）时，对应需传递的文件（夹）id
   */
  delete_file_id: string;
  move: number;
  /**
   * 任务状态：-1下载失败；0分配中；1下载中；2下载成功
   */
  status: -1 | 0 | 1 | 2;
  /** 服务端提供的显示文案和进度；旧响应可能不返回。 */
  status_text?: string;
  display_percent?: number;
  /**
   * 链接任务url
   */
  url: string;
  del_path: string;
  /**
   * 任务源文件所在父文件夹id
   */
  wp_path_id: string;
  /**
   * 视频清晰度；1:标清 2:高清 3:超清 4:1080P 5:4k;100:原画
   */
  def2: number;
  /**
   * 视频时长
   */
  play_long: number;
  /**
   * 是否可申诉
   */
  can_appeal: number;
}

export interface QuotaInfoResponseData {
  /**
   * 配额类型
   */
  package?: Package[];
  /**
   * 用户总配额数量
   */
  count: number;
  /**
   * 用户总剩余配额数量
   */
  surplus?: number;
  max_size?: number;
  /**
   * 用户总已用配额数量
   */
  used: number;
}

export interface Package {
  /**
   * 该类型剩余配额
   */
  surplus: number;
  /**
   * 该类型已用配额
   */
  used: number;
  /**
   * 该类型总配额
   */
  count: number;
  /**
   * 该类型配额名称
   */
  name: string;
  /**
   * 该类型明细项过期信息
   */
  expire_info: ExpireInfo[] | null;
}

export interface ExpireInfo {
  /**
   * 明细项剩余配额
   */
  surplus: number;
  /**
   * 明细项过期时间
   */
  expire_time: number;
}

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
  /**
   * 删除任务需删除源文件（夹）时，对应需传递的文件（夹）id
   */
  delete_file_id: string;
  move: number;
  /**
   * 任务状态：-1下载失败；0分配中；1下载中；2下载成功
   */
  status: -1 | 0 | 1 | 2;
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

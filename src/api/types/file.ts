import type { ResponseData } from '@/utils/http/alova';

export interface FileListRequestParams {
  cid?: string;
  type?: number;
  limit?: number;
  offset?: number;
  suffix?: string;
  asc?: number;
  o?: string;
  custom_order?: number;
  stdir?: number;
  star?: number;
  /**
   * 是否只显示当前文件夹内文件，1：只显示当前文件夹内文件，0：显示所有文件
   */
  cur?: number;
  show_dir?: number;
  /**
   * 是否不显示文件，1：不显示，0：显示
   */
  nf?: number;
}

export interface FileListResponseData extends ResponseData<MyFile[]> {
  /**
   * 文件的状态，1 正常，7 删除(回收站)，120 彻底删除
   */
  aid: string;
  /**
   * 父目录id
   */
  cid: string;
  /**
   * 当前目录文件数量
   */
  count: number;
  cur: number;
  fields: string;
  /**
   * 是否返回文件数据
   */
  hide_data: string;
  /**
   * 排序，1：升序 0：降序
   */
  is_asc: number;
  /**
   * 分页量
   */
  limit: number;
  max_size: number;
  min_size: number;
  /**
   * 偏移量
   */
  offset: number;
  /**
   * 排序
   */
  order: string;
  /**
   * 父目录树
   */
  path: Path[];
  /**
   * 是否记录文件夹的打开时间
   */
  record_open_time: string;
  /**
   * 是否星标；1：星标；0：未星标
   */
  star: number;
  stdir: number;
  /**
   * 一级筛选选其他时填写的后缀名
   */
  suffix: string;
  /**
   * 系统文件夹数量
   */
  sys_count: number;
  sys_dir: string;
  /**
   *  一级筛选大分类，1：文档，2：图片，3：音乐，4：视频，5：压缩包，6：应用，7：书籍
   */
  type: number;
}

export interface Path {
  name: string;
  cid: string;
  pid: string;
  isp: string;
  p_cid: string;
  fv: string;
}

export interface MyFile {
  /**
   * 文件的状态，1 正常，7 删除(回收站)，120 彻底删除
   */
  aid: string;
  cm: number;
  /**
   * 视频清晰度，1:标清，2:高清，3:超清，4:1080P，5:4k，100:原画
   */
  def: number;
  /**
   * 视频清晰度，1:标清，2:高清，3:超清，4:1080P，5:4k，100:原画
   */
  def2: number;
  /**
   * 音频长度
   */
  fatr: string;
  /**
   * 文件分类：0 文件夹，1 文件
   */
  fc: string;
  /**
   * 文件夹封面
   */
  fco: string;
  /**
   * 文件备注
   */
  fdesc: string;
  /**
   * 文件id
   */
  fid: string;
  /**
   * 文件标签
   */
  fl: FileLabel[];
  /**
   * 文件(夹)名称
   */
  fn: string;
  /**
   * 文件大小
   */
  fs: number;
  /**
   * 文件状态 0/2 未上传完成，1 已上传完成
   */
  fta: string;
  /**
   * 官方文档未给说明，猜测和文件后缀名有关
   */
  ftype: string;
  /**
   * 文件所有者id
   */
  fuuid: number;
  fvs: number;
  ic: string;
  /**
   * 文件后缀名
   */
  ico: string;
  /**
   * 是否置顶
   */
  is_top: number;
  /**
   * 是否星标，1：星标
   */
  ism: string;
  /**
   * 是否加密；1：加密
   */
  isp: number;
  /**
   * 是否统计文件夹下视频时长开关
   */
  ispl: number;
  iss: number;
  issct: number;
  /**
   * 是否为视频
   */
  isv: number;
  multitrack: number;
  /**
   * 上次打开时间
   */
  opt: number;
  /**
   * 文件提取码
   */
  pc: string;
  /**
   * 父目录ID
   */
  pid: string;
  /**
   * 音视频时长
   */
  play_long: number;
  /**
   * sha1值
   */
  sha1: string;
  /**
   * 修改时间
   */
  uet: number;
  /**
   * 上传时间
   */
  uppt: number;
  /**
   * 修改时间
   */
  upt: number;
  v_img: string;
}

export interface FileLabel {
  /**
   * 文件标签id
   */
  id: string;
  /**
   * 文件标签名称
   */
  name: string;
  /**
   * 文件标签排序
   */
  sort: string;
  /**
   * 文件标签颜色
   */
  color: string;
  /**
   * 文件标签类型；0：最近使用；1：非最近使用；2：为默认标签
   */
  is_default: number;
  /**
   * 文件标签更新时间
   */
  update_time: number;
  /**
   * 文件标签创建时间
   */
  create_time: number;
}

export interface FileDetail {
  /**
   * 包含文件总数量
   */
  count: string;
  /**
   * 文件(夹)总大小
   */
  size: string;
  /**
   * 包含文件夹总数量
   */
  folder_count: string;
  /**
   * 视频时长；-1：正在统计，其他数值为视频时长的数值(单位秒)
   */
  play_long: number;
  /**
   * 是否开启展示视频时长
   */
  show_play_long: number;
  /**
   * 上传时间
   */
  ptime: string;
  /**
   * 修改时间
   */
  utime: string;
  /**
   * 文件名
   */
  file_name: string;
  /**
   * 文件提取码
   */
  pick_code: string;
  /**
   * sha1值
   */
  sha1: string;
  /**
   * 文件(夹)ID
   */
  file_id: string;
  /**
   * 是否星标
   */
  is_mark: string;
  /**
   * 文件(夹)最近打开时间
   */
  open_time: number;
  /**
   * 文件属性；1：文件；0：文件夹
   */
  file_category: string;
  /**
   * 文件(夹)所在的路径
   */
  paths: {
    /**
     * 父目录ID
     */
    file_id: string;
    /**
     * 父目录名称
     */
    file_name: string;
  }[];
}

export type RecycleBinListResponseData = {
  offset: number;
  limit: number;
  count: string;
  rb_pass: number;
} & Record<string, RecycleBinFile>;

export interface RecycleBinFile {
  id: string;
  file_name: string;
  /**
   * 类型（1：文件，2：目录
   */
  type: string;
  file_size: string;
  dtime: string;
  thumb_url: string;
  status: string;
  cid: string;
  parent_name: string;
  pick_code: string;
  ico: string;
}

export interface FileDownloadUrlResponseData {
  [key: string]: {
    file_name: string;
    file_size: number;
    pick_code: string;
    sha1: string;
    url: {
      url: string;
    };
  };
}

export interface FileSearchRequestParams {
  /**
   * 查找关键字
   */
  search_value: string;
  /**
   * 单页记录数，默认20，offset+limit最大不超过10000
   */
  limit: number;
  /**
   * 数据显示偏移量
   */
  offset: number;
  /**
   * 支持文件标签搜索
   */
  file_label?: number;
  /**
   * 目标目录cid=-1时，表示不返回列表任何内容
   */
  cid?: string;
  /**
   * 搜索结果匹配的开始时间；格式：2020-11-19
   */
  gte_day?: string;
  /**
   * 搜索结果匹配的结束时间；格式：2020-11-20
   */
  lte_day?: string;
  /**
   * 只显示文件或文件夹。1 只显示文件夹，2 只显示文件
   */
  fc?: number;
  /**
   * 一级筛选大分类，1：文档，2：图片，3：音乐，4：视频，5：压缩包，6：应用
   */
  type?: 0 | 1 | 2 | 3 | 4 | 5 | 6;
  /**
   * 一级筛选选其他时填写的后缀名
   */
  suffix?: string;
}

export interface FileSearchResponseData extends ResponseData<SearchFile[]> {
  limit: number;
  offset: number;
  count: number;
}

export interface SearchFile {
  /**
   * 文件ID
   */
  file_id: string;
  /**
   * 文件的状态，aid 的别名。1 正常，7 删除(回收站)，120 彻底删除
   */
  area_id: string;
  /**
   * 父目录ID
   */
  parent_id: string;
  /**
   * 用户ID
   */
  user_id: string;
  /**
   * 1：文件；0；文件夹
   */
  file_category: '0' | '1';
  /**
   * 文件名称
   */
  file_name: string;
  /**
   * 文件是否隐藏。0 未隐藏，1 已隐藏
   */
  is_private: 0 | 1;
  /**
   * 文件提取码
   */
  pick_code: string;
  /**
   * 上传时间
   */
  user_ptime: string;
  /**
   * 更新时间
   */
  user_utime: string;
  /**
   * 文件sha1值
   */
  sha1?: string;
  /**
   * 文件大小
   */
  file_size?: string;
  /**
   * 文件后缀
   */
  ico?: string;
}

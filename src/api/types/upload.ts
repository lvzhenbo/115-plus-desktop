/** 上传初始化请求参数 */
export interface UploadInitParams {
  /** 文件名 */
  file_name: string;
  /** 文件大小(字节) */
  file_size: number;
  /** 文件上传目标约定 U_1_{cid}，0代表根目录 */
  target: string;
  /** 文件sha1值 */
  fileid: string;
  /** 文件前128K sha1 */
  preid?: string;
  /** 上传任务key（续传时需要） */
  pick_code?: string;
  /** 上传调度文件类型标记 */
  topupload?: number;
  /** 二次认证 sign_key */
  sign_key?: string;
  /** 二次认证 sign_val */
  sign_val?: string;
}

/** 上传初始化回调信息 */
export interface UploadCallback {
  callback: string;
  callback_var: string;
}

/** 上传初始化响应数据项 */
export interface UploadInitDataItem {
  /** 上传任务唯一ID，用于续传 */
  pick_code: string;
  /** 上传状态；1：非秒传；2：秒传 */
  status: number;
  /** sha1 标识（二次认证） */
  sign_key?: string;
  /** 计算本地文件sha1区间范围（二次认证） */
  sign_check?: string;
  /** 秒传成功返回的新增文件ID */
  file_id?: string;
  /** 文件上传目标约定 */
  target?: string;
  /** 上传的bucket */
  bucket?: string;
  /** OSS objectID */
  object?: string;
  /** 上传回调信息 */
  callback?: UploadCallback;
}

/** 获取上传凭证响应数据 */
export interface UploadTokenData {
  /** 上传域名 */
  endpoint: string;
  /** 上传凭证-密钥 */
  AccessKeySecret: string;
  /** 上传凭证-token */
  SecurityToken: string;
  /** 上传凭证-过期日期 */
  Expiration: string;
  /** 上传凭证-ID */
  AccessKeyId: string;
}

/** 断点续传请求参数 */
export interface UploadResumeParams {
  /** 文件大小(字节) */
  file_size: number;
  /** 文件上传目标约定 */
  target: string;
  /** 文件sha1值 */
  fileid: string;
  /** 上传任务key */
  pick_code: string;
}

/** 断点续传响应数据项 */
export interface UploadResumeDataItem {
  /** 上传任务唯一ID */
  pick_code: string;
  /** 文件上传目标约定 */
  target: string;
  /** 接口版本 */
  version?: string;
  /** 上传的bucket */
  bucket: string;
  /** OSS objectID */
  object: string;
  /** 上传回调信息 */
  callback: UploadCallback;
}

export interface AuthDeviceCodeRequestData {
  client_id: string;
  code_challenge: string;
  code_challenge_method: 'sha256' | 'md5' | 'sha1';
}

export interface AuthDeviceCodeResponseData {
  uid: string;
  time: number;
  qrcode: string;
  sign: string;
}

export interface QrCodeStatusRequestParams {
  uid: string;
  time: number;
  sign: string;
}

export interface QrCodeStatusResponseData {
  /**
   * 1: 扫码成功，等待确认;
   *
   * 2: 确认登录/授权，结束轮询;
   *
   * -2: 取消登录;
   */
  status: 1 | 2 | -2;
  msg: string;
  version: string;
}

export interface DeviceCodeToTokenRequestData {
  uid: string;
  code_verifier: string;
}

export interface DeviceCodeToTokenResponseData {
  access_token: string;
  refresh_token: string;
  expires_in: number;
}

export interface RefreshTokenRequestData {
  refresh_token: string;
}

export interface UserInfoResponseData {
  user_id: number;
  user_name: string;
  user_face_s: string;
  user_face_m: string;
  user_face_l: string;
  rt_space_info: RtSpaceInfo;
  vip_info: VipInfo;
}

export interface RtSpaceInfo {
  all_total: All;
  all_remain: All;
  all_use: All;
}

export interface All {
  size: number;
  size_format: string;
}

export interface VipInfo {
  expire: number;
  level_name: string;
  tp_rights: TpRights;
}

export interface TpRights {
  is_tp_rights: number;
  tp_rights_time: number;
}

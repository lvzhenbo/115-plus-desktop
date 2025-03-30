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

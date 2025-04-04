import { alovaInst, type ResponseData } from '@/utils/http/alova';
import { loginBaseUrl, openBaseUrl, qrcodeBaseUrl } from './config';
import type {
  AuthDeviceCodeRequestData,
  AuthDeviceCodeResponseData,
  DeviceCodeToTokenRequestData,
  DeviceCodeToTokenResponseData,
  QrCodeStatusRequestParams,
  QrCodeStatusResponseData,
  RefreshTokenRequestData,
  UserInfoResponseData,
} from './types/user';

export const authDeviceCode = (data: AuthDeviceCodeRequestData) =>
  alovaInst.Post<ResponseData<AuthDeviceCodeResponseData>>(
    `${loginBaseUrl}/open/authDeviceCode`,
    data,
    {
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
      },
      meta: {
        authRole: null,
      },
    },
  );

export const qrCodeStatus = (params: QrCodeStatusRequestParams) =>
  alovaInst.Get<ResponseData<QrCodeStatusResponseData>>(`${qrcodeBaseUrl}/get/status/`, {
    params,
    cacheFor: null,
    meta: {
      authRole: null,
    },
  });

export const deviceCodeToToken = (data: DeviceCodeToTokenRequestData) =>
  alovaInst.Post<ResponseData<DeviceCodeToTokenResponseData>>(
    `${loginBaseUrl}/open/deviceCodeToToken`,
    data,
    {
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
      },
      meta: {
        authRole: 'login',
      },
    },
  );

export const refreshToken = (data: RefreshTokenRequestData) =>
  alovaInst.Post<ResponseData<DeviceCodeToTokenResponseData>>(
    `${loginBaseUrl}/open/refreshToken`,
    data,
    {
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
      },
      meta: {
        authRole: 'refreshToken',
      },
    },
  );

export const userInfo = () =>
  alovaInst.Get<ResponseData<UserInfoResponseData>>(`${openBaseUrl}/open/user/info`, {
    cacheFor: null,
  });

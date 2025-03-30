import { alovaInst, type ResponseData } from '@/utils/http/alova';
import { loginBaseUrl, qrcodeBaseUrl } from './config';
import type {
  AuthDeviceCodeRequestData,
  AuthDeviceCodeResponseData,
  DeviceCodeToTokenRequestData,
  DeviceCodeToTokenResponseData,
  QrCodeStatusRequestParams,
  QrCodeStatusResponseData,
} from './types/user';

export const authDeviceCode = (data: AuthDeviceCodeRequestData) =>
  alovaInst.Post<ResponseData<AuthDeviceCodeResponseData>>(
    `${loginBaseUrl}/open/authDeviceCode`,
    data,
    {
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
      },
    },
  );

export const qrCodeStatus = (params: QrCodeStatusRequestParams) =>
  alovaInst.Get<ResponseData<QrCodeStatusResponseData>>(`${qrcodeBaseUrl}/get/status/`, {
    params,
    cacheFor: null,
  });

export const deviceCodeToToken = (data: DeviceCodeToTokenRequestData) =>
  alovaInst.Post<ResponseData<DeviceCodeToTokenResponseData>>(
    `${loginBaseUrl}/open/deviceCodeToToken`,
    data,
    {
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
      },
    },
  );

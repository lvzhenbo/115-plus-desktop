import { createAlova } from 'alova';
import adapterTauriFetch from './tauriHttpAdapter';
import { useMessage } from '@/composables/useDiscreteApi';
import { createServerTokenAuthentication } from 'alova/client';
import { useUserStoreWithOut } from '@/store/user';
import type { DeviceCodeToTokenResponseData } from '@/api/types/user';
import { refreshToken } from '@/api/user';
import adapterFetch from 'alova/fetch';
import { useSettingStoreWithOut } from '@/store/setting';
import { emit } from '@tauri-apps/api/event';

export interface ResponseData<T> {
  state: 0 | 1 | boolean;
  code: number;
  message: string;
  data: T;
  error?: string;
  errno?: number;
}

const message = useMessage();
const userStore = useUserStoreWithOut();
const settingStore = useSettingStoreWithOut();

const { onAuthRequired, onResponseRefreshToken } = createServerTokenAuthentication({
  refreshTokenOnSuccess: {
    isExpired: async (response, _method) => {
      if (response.status >= 400) {
        throw new Error(response.statusText);
      }
      const json: ResponseData<unknown> = await response.clone().json();
      return json.code === 40140125 || json.code === 40140121;
    },
    handler: async (_response, _method) => {
      try {
        const res = await refreshToken({
          refresh_token: userStore.refreshToken,
        });
        userStore.accessToken = res.data.access_token;
        userStore.refreshToken = res.data.refresh_token;
        userStore.expiresIn = res.data.expires_in;
        emit('token-updated').catch(console.error);
      } catch (error) {
        message.error('登录失效，请重新登录');
        userStore.logout();
        console.error(error);
        throw error;
      }
    },
  },
  assignToken: (method) => {
    method.config.headers.Authorization = `Bearer ${userStore.accessToken}`;
  },
  login: async (response, _method) => {
    const json: ResponseData<DeviceCodeToTokenResponseData> = await response.clone().json();
    userStore.accessToken = json.data.access_token;
    userStore.refreshToken = json.data.refresh_token;
    userStore.expiresIn = json.data.expires_in;
  },
});

export const alovaInst = createAlova({
  requestAdapter: adapterTauriFetch(),
  timeout: 40000,
  beforeRequest: onAuthRequired((method) => {
    console.log(method);
  }),
  responded: onResponseRefreshToken({
    onSuccess: async (response, _method) => {
      if (response.status >= 400) {
        throw new Error(response.statusText);
      }
      const json: ResponseData<unknown> = await response.clone().json();
      console.log(json);

      if (!json.state) {
        if (json.code === 40199002) {
          message.error('二维码已失效，请重新扫码');
        } else if (json.code === 40140116 || json.code === 40140119) {
          message.error('登录失效，请重新登录');
          userStore.logout();
        } else {
          message.error(json.message);
        }
        throw json;
      }
      return json;
    },
    onError: (err, _method) => {
      message.error(JSON.stringify(err));
    },
    onComplete: async (_method) => {
      // 处理请求完成逻辑
    },
  }),
});

export const aria2Server = createAlova({
  requestAdapter: adapterFetch(),
  baseURL: `http://localhost:${settingStore.downloadSetting.aria2Port}`,
  timeout: 40000,
  responded: {
    onSuccess: async (response, _method) => {
      if (response.status !== 200) {
        throw new Error(response.statusText);
      }
      const json: ResponseData<unknown> = await response.clone().json();
      return json;
    },
  },
});
